//! Dynamic module support — GNU Emacs compatible dynamic module API.
//!
//! Architecture mirrors GNU `src/emacs-module.c`.

#![allow(non_camel_case_types)]
#![allow(unsafe_op_in_unsafe_fn)]

use crate::emacs_core::process::SystemProcessExt;
use malachite::integer::Integer;
use neovm_eval::emacs_core::eval::data::dynamic_module_registry::{
    GlobalRefEntry, GlobalRefMetadata, dynamic_module_registry_heap_identity, emacs_value,
    emacs_value_tag, with_global_refs,
};
use neovm_value::error::LispCondition;
use std::collections::HashMap;
use std::ffi::{CStr, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Mutex, PoisonError};

use libloading::Library;

use super::timefns::{LispTimeOutput, make_lisp_time};
use neovm_eval::emacs_core::eval::Context;
use neovm_value::error::{EvalResult, Flow, FlowKind, signal};
use neovm_value::heap_types::LispString;
use neovm_value::intern::{intern, intern_lisp_string, resolve_sym};
use neovm_value::tagged::gc::{HeapWriteKind, note_heap_write};
use neovm_value::tagged::header::{ModuleFunctionObj, UserPtrObj, VecLikeType};
use neovm_value::value::Value;

// ============================================================================
// C-compatible types matching GNU emacs-module.h
// ============================================================================

pub(crate) type emacs_function = unsafe extern "C" fn(
    env: *mut emacs_env,
    nargs: isize,
    args: *mut emacs_value,
    data: *mut c_void,
) -> emacs_value;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum emacs_funcall_exit {
    Return = 0,
    Signal = 1,
    Throw = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum emacs_process_input_result {
    Continue = 0,
    Quit = 1,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct emacs_time {
    pub tv_sec: libc::time_t,
    pub tv_nsec: libc::c_long,
}

pub(crate) type emacs_limb_t = u64;

#[allow(non_upper_case_globals)]
pub(crate) const emacs_variadic_function: isize = -2;

// ============================================================================
// emacs_env — the vtable struct
// ============================================================================

#[repr(C)]
pub struct emacs_env {
    pub size: isize,
    pub(crate) private_members: *mut emacs_env_private,

    // Memory management
    pub(crate) make_global_ref:
        Option<unsafe extern "C" fn(env: *mut emacs_env, value: emacs_value) -> emacs_value>,
    pub(crate) free_global_ref:
        Option<unsafe extern "C" fn(env: *mut emacs_env, global_value: emacs_value)>,

    // Non-local exits
    pub(crate) non_local_exit_check:
        Option<unsafe extern "C" fn(env: *mut emacs_env) -> emacs_funcall_exit>,
    pub(crate) non_local_exit_clear: Option<unsafe extern "C" fn(env: *mut emacs_env)>,
    pub(crate) non_local_exit_get: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            symbol: *mut emacs_value,
            data: *mut emacs_value,
        ) -> emacs_funcall_exit,
    >,
    pub(crate) non_local_exit_signal:
        Option<unsafe extern "C" fn(env: *mut emacs_env, symbol: emacs_value, data: emacs_value)>,
    pub(crate) non_local_exit_throw:
        Option<unsafe extern "C" fn(env: *mut emacs_env, tag: emacs_value, value: emacs_value)>,

    // Function registration
    pub(crate) make_function: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            min_arity: isize,
            max_arity: isize,
            func: emacs_function,
            docstring: *const std::ffi::c_char,
            data: *mut c_void,
        ) -> emacs_value,
    >,
    pub funcall: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            func: emacs_value,
            nargs: isize,
            args: *mut emacs_value,
        ) -> emacs_value,
    >,
    pub intern: Option<
        unsafe extern "C" fn(env: *mut emacs_env, name: *const std::ffi::c_char) -> emacs_value,
    >,

    // Type conversion
    pub type_of: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> emacs_value>,
    pub(crate) is_not_nil:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> bool>,
    pub eq:
        Option<unsafe extern "C" fn(env: *mut emacs_env, a: emacs_value, b: emacs_value) -> bool>,
    pub(crate) extract_integer:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> i64>,
    pub make_integer: Option<unsafe extern "C" fn(env: *mut emacs_env, n: i64) -> emacs_value>,
    pub extract_float: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> f64>,
    pub make_float: Option<unsafe extern "C" fn(env: *mut emacs_env, d: f64) -> emacs_value>,

    // Strings
    pub(crate) copy_string_contents: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            value: emacs_value,
            buf: *mut std::ffi::c_char,
            len: *mut isize,
        ) -> bool,
    >,
    pub make_string: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            str: *const std::ffi::c_char,
            len: isize,
        ) -> emacs_value,
    >,
    // User pointer
    pub(crate) make_user_ptr: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            fin: Option<unsafe extern "C" fn(*mut c_void)>,
            ptr: *mut c_void,
        ) -> emacs_value,
    >,
    pub(crate) get_user_ptr:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> *mut c_void>,
    pub(crate) set_user_ptr:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value, ptr: *mut c_void)>,
    pub(crate) get_user_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            uptr: emacs_value,
        ) -> Option<unsafe extern "C" fn(*mut c_void)>,
    >,
    pub(crate) set_user_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
            fin: Option<unsafe extern "C" fn(*mut c_void)>,
        ),
    >,

    // Vectors
    pub(crate) vec_get: Option<
        unsafe extern "C" fn(env: *mut emacs_env, vector: emacs_value, index: isize) -> emacs_value,
    >,
    pub(crate) vec_set: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            vector: emacs_value,
            index: isize,
            value: emacs_value,
        ),
    >,
    pub(crate) vec_size:
        Option<unsafe extern "C" fn(env: *mut emacs_env, vector: emacs_value) -> isize>,

    // Quit/input
    pub(crate) should_quit: Option<unsafe extern "C" fn(env: *mut emacs_env) -> bool>,
    pub(crate) process_input:
        Option<unsafe extern "C" fn(env: *mut emacs_env) -> emacs_process_input_result>,

    // Time
    pub(crate) extract_time:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> emacs_time>,
    pub(crate) make_time:
        Option<unsafe extern "C" fn(env: *mut emacs_env, time: emacs_time) -> emacs_value>,

    // Big integer
    pub(crate) extract_big_integer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
            sign: *mut std::ffi::c_int,
            count: *mut isize,
            magnitude: *mut emacs_limb_t,
        ) -> bool,
    >,
    pub(crate) make_big_integer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            sign: std::ffi::c_int,
            count: isize,
            magnitude: *const emacs_limb_t,
        ) -> emacs_value,
    >,

    // Function finalizer
    pub(crate) get_function_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
        ) -> Option<unsafe extern "C" fn(*mut c_void)>,
    >,
    pub(crate) set_function_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
            fin: Option<unsafe extern "C" fn(*mut c_void)>,
        ),
    >,

    // Pipe channel
    pub open_channel: Option<
        unsafe extern "C" fn(env: *mut emacs_env, pipe_process: emacs_value) -> std::ffi::c_int,
    >,

    // Interactive
    pub(crate) make_interactive:
        Option<unsafe extern "C" fn(env: *mut emacs_env, function: emacs_value, spec: emacs_value)>,

    // Unibyte string, added at the end of GNU emacs_env_31.
    pub(crate) make_unibyte_string: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            str: *const std::ffi::c_char,
            len: isize,
        ) -> emacs_value,
    >,
}

// ============================================================================
// Runtime structs
// ============================================================================

pub(crate) struct emacs_runtime_private {
    pub env: *mut emacs_env,
}

#[repr(C)]
pub(crate) struct emacs_runtime {
    pub size: isize,
    pub(crate) private_members: *mut emacs_runtime_private,
    pub(crate) get_environment:
        Option<unsafe extern "C" fn(runtime: *mut emacs_runtime) -> *mut emacs_env>,
}

// ============================================================================
// Value arena
// ============================================================================

#[cfg_attr(feature = "test-support", visibility::make(pub))]
const VALUE_FRAME_SIZE: usize = 512;

#[repr(C)]
pub(crate) struct emacs_value_frame {
    pub objects: [emacs_value_tag; VALUE_FRAME_SIZE],
    pub offset: usize,
    pub next: Option<std::ptr::NonNull<emacs_value_frame>>,
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct emacs_value_storage {
    initial: emacs_value_frame,
    /// Head of the heap-allocated overflow frame chain, or `None` while
    /// allocations still fit in `initial`. Never points at `self.initial`, so
    /// `emacs_value_storage` is never self-referential and is ready immediately
    /// after `new()`.
    overflow: Option<std::ptr::NonNull<emacs_value_frame>>,
}

// ============================================================================
// emacs_env_private
// ============================================================================

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct emacs_env_private {
    pub pending_non_local_exit: emacs_funcall_exit,
    pub non_local_exit_symbol: Value,
    pub non_local_exit_data: Value,
    pub storage: emacs_value_storage,
}

// ============================================================================
// Global module state
// ============================================================================

/// A module's native code remains mapped for the process lifetime, including
/// failed initialization, just as GNU never calls dynlib_close on a module.
/// Only shared references escape `open`; no safe owner can unload the library.
/// This immutable loader handle is shared across mutators and contains no Lisp
/// state. The loader supplies synchronization for symbol lookup.
#[derive(Debug)]
struct ModuleLibrary {
    library: Library,
}

static_assertions::assert_impl_all!(ModuleLibrary: Send, Sync);

#[derive(Debug, thiserror::Error)]
enum ModuleOpenError {
    #[error(transparent)]
    Loader(#[from] libloading::Error),
}

impl ModuleLibrary {
    #[cold]
    #[inline(never)]
    pub fn open(path: &std::path::Path) -> Result<&'static Self, ModuleOpenError> {
        // SAFETY: native modules follow GNU's module-load contract: their
        // constructors and exported entry points implement the Emacs module
        // ABI. The handle is retained before any exported code is called.
        let library = unsafe { Library::new(path) }?;
        Ok(Box::leak(Box::new(Self { library })))
    }
}

/// A loaded module's shared library, kept mapped until the process exits or
/// the same path loads again.
///
/// Only the library outlives `module-load`. GNU's `Fmodule_load` keeps the
/// runtime and its initialization environment on the C stack, valid only while
/// the module's init function runs, so they are freed when loading returns,
/// as on the failure paths. The environment holds evaluator-local values; it
/// never belonged in this process-wide registry.
pub(crate) struct LoadedModule {
    #[allow(dead_code)]
    library: &'static ModuleLibrary,
}

static_assertions::assert_impl_all!(LoadedModule: Send, Sync);

static LOADED_MODULES: Mutex<Option<HashMap<String, LoadedModule>>> = Mutex::new(None);

thread_local! {
    static ACTIVE_ENVS: std::cell::RefCell<Vec<ActiveModuleEnvEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

mod source_scope;

#[doc(hidden)]
pub use source_scope::register_source_module_scope_probe;

#[derive(Clone, Copy)]
struct ActiveModuleEnvEntry {
    env_priv: *mut emacs_env_private,
    heap_identity: usize,
}

// ============================================================================
// Value arena implementation
// ============================================================================

impl emacs_value_frame {
    fn new() -> Self {
        Self {
            // SAFETY: `emacs_value_tag` wraps `Value`, whose all-zero bit
            // pattern is a valid representation (`Value::NIL` / fixnum 0
            // slots are overwritten before `offset` advances).
            objects: unsafe { std::mem::zeroed() },
            offset: 0,
            next: None,
        }
    }
}

impl emacs_value_storage {
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn new() -> Self {
        Self {
            initial: emacs_value_frame::new(),
            overflow: None,
        }
    }

    fn reset(&mut self) {
        finalize_storage(self);
    }
}

impl Drop for emacs_value_storage {
    fn drop(&mut self) {
        finalize_storage(self);
    }
}

impl emacs_env_private {
    fn new_boxed() -> Box<Self> {
        Box::new(Self {
            pending_non_local_exit: emacs_funcall_exit::Return,
            non_local_exit_symbol: Value::NIL,
            non_local_exit_data: Value::NIL,
            storage: emacs_value_storage::new(),
        })
    }
}

fn allocate_emacs_value(storage: &mut emacs_value_storage, lisp_val: Value) -> emacs_value {
    let storage_ptr = std::ptr::from_mut(storage);
    // SAFETY: `storage_ptr` addresses the exclusively borrowed `storage`, and
    // any `Some(ptr)` in `overflow` is an exclusively owned heap frame created
    // via `Box::leak`. Deriving the active frame via raw pointers avoids
    // forming an overlapping `&mut storage.initial` across allocations that
    // would invalidate previously returned `emacs_value` handles in `initial`.
    let active_ptr: *mut emacs_value_frame = match unsafe { (*storage_ptr).overflow } {
        Some(ptr) => ptr.as_ptr(),
        None => unsafe { std::ptr::addr_of_mut!((*storage_ptr).initial) },
    };
    // SAFETY: `active_ptr` points to either `storage.initial` or the head
    // overflow frame, both live and aligned for this call.
    let offset = unsafe { (*active_ptr).offset };
    if offset < VALUE_FRAME_SIZE {
        // SAFETY: `offset < VALUE_FRAME_SIZE`, so `objects[offset]` is in
        // bounds of the live active frame and exclusively written here.
        unsafe {
            let slot = std::ptr::addr_of_mut!((*active_ptr).objects[offset]);
            slot.write(emacs_value_tag::new(lisp_val));
            (*active_ptr).offset = offset + 1;
            slot
        }
    } else {
        let mut new_frame = Box::new(emacs_value_frame::new());
        // SAFETY: `storage_ptr` is live and exclusively borrowed.
        new_frame.next = unsafe { (*storage_ptr).overflow };
        new_frame.objects[0] = emacs_value_tag::new(lisp_val);
        new_frame.offset = 1;
        let new_ptr = std::ptr::NonNull::from(Box::leak(new_frame));
        // SAFETY: `storage_ptr` is live and exclusively borrowed, and `new_ptr`
        // points to the heap frame just linked into `storage.overflow`.
        unsafe {
            (*storage_ptr).overflow = Some(new_ptr);
            std::ptr::addr_of_mut!((*new_ptr.as_ptr()).objects[0])
        }
    }
}

/// # Safety
/// `v` must be null or point to a live `emacs_value_tag` allocated by this
/// runtime's module storage or global-ref registry.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe fn value_to_lisp(v: emacs_value) -> Value {
    if v.is_null() {
        return Value::NIL;
    }
    unsafe { (*v).v }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn finalize_storage(storage: &mut emacs_value_storage) {
    // Overflow frames form a singly-linked heap chain terminated by `None`;
    // `storage.initial` lives inline and is never on the chain.
    let mut frame = storage.overflow.take();
    while let Some(ptr) = frame {
        // SAFETY: every node in `storage.overflow` was created via
        // `Box::leak(Box::new(emacs_value_frame::new()))` and is exclusively
        // owned by `storage`.
        let boxed = unsafe { Box::from_raw(ptr.as_ptr()) };
        frame = boxed.next;
        drop(boxed);
    }
    storage.initial.next = None;
    storage.initial.offset = 0;
}

impl Drop for emacs_env_private {
    fn drop(&mut self) {
        // Initialization failures and contained panics also retire local tags.
        finalize_storage(&mut self.storage);
    }
}

// ============================================================================
// Module → Value helpers
// ============================================================================

/// # Safety
/// `env` must be null or a live `emacs_env` with valid `private_members`
/// exclusively accessed on the current thread.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe fn lisp_to_value(env: *mut emacs_env, val: Value) -> emacs_value {
    if env.is_null() {
        return std::ptr::null_mut::<emacs_value_tag>();
    }
    unsafe {
        let priv_ = &mut *(*env).private_members;
        if priv_.pending_non_local_exit != emacs_funcall_exit::Return {
            return std::ptr::null_mut::<emacs_value_tag>();
        }
        allocate_emacs_value(&mut priv_.storage, val)
    }
}

/// # Safety
/// `env` must be null or a live `emacs_env` with valid `private_members`
/// exclusively accessed on the current thread.
unsafe fn lisp_to_value_ignoring_pending_exit(env: *mut emacs_env, val: Value) -> emacs_value {
    if env.is_null() {
        return std::ptr::null_mut::<emacs_value_tag>();
    }
    unsafe {
        let priv_ = &mut *(*env).private_members;
        allocate_emacs_value(&mut priv_.storage, val)
    }
}

// ============================================================================
// Non-local exits
// ============================================================================

/// # Safety
/// `env` must be null or a live `emacs_env` with valid `private_members`.
unsafe fn check_pending_non_local_exit(env: *mut emacs_env) -> bool {
    if env.is_null() {
        return false;
    }
    unsafe { (*env).private_members.as_ref() }
        .map(|p| p.pending_non_local_exit != emacs_funcall_exit::Return)
        .unwrap_or(false)
}

/// # Safety
/// `env` must be null or a live `emacs_env` with valid `private_members`.
unsafe fn module_function_begin(env: *mut emacs_env) -> bool {
    // SAFETY: `env` is either null-checked above or the live `emacs_env` for this FFI call.
    !env.is_null() && !unsafe { check_pending_non_local_exit(env) }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe fn set_pending_signal(env: *mut emacs_env, symbol: &str, data: Value) {
    if env.is_null() {
        return;
    }
    unsafe {
        let priv_ = &mut *(*env).private_members;
        if priv_.pending_non_local_exit == emacs_funcall_exit::Return {
            priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
            priv_.non_local_exit_symbol = Value::from_sym_id(intern(symbol));
            priv_.non_local_exit_data = data;
        }
    }
}

/// Whether a module call is in progress on this thread.
#[doc(hidden)]
pub fn module_call_active() -> bool {
    ACTIVE_ENVS.with(|envs| !envs.borrow().is_empty())
}

/// Active environment frames are scoped to an in-progress native call.
/// Global references are traced separately from the Context-owned registry.
#[doc(hidden)]
pub fn collect_dynamic_module_gc_roots(roots: &mut Vec<Value>, heap_identity: usize) {
    ACTIVE_ENVS.with(|envs| {
        for entry in envs.borrow().iter() {
            // Check ownership before dereferencing the borrowed environment.
            if entry.heap_identity != heap_identity || entry.env_priv.is_null() {
                continue;
            }
            // SAFETY: `entry.env_priv` is kept live by `ActiveModuleEnv` while
            // on `ACTIVE_ENVS`, and `overflow` chains exclusively owned heap frames.
            unsafe {
                let priv_ = &*entry.env_priv;
                roots.push(priv_.non_local_exit_symbol);
                roots.push(priv_.non_local_exit_data);
                let mut frame = priv_.storage.overflow;
                while let Some(ptr) = frame {
                    let frame_ref = ptr.as_ref();
                    for item in frame_ref.objects.iter().take(frame_ref.offset) {
                        roots.push(item.v);
                    }
                    frame = frame_ref.next;
                }
                for item in priv_
                    .storage
                    .initial
                    .objects
                    .iter()
                    .take(priv_.storage.initial.offset)
                {
                    roots.push(item.v);
                }
            }
        }
    });
}

#[must_use = "the thread-local extent ends when this guard drops"]
#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
struct ActiveModuleEnv {
    _scope: neovm_value::tls_scope::TlsStackScope<ActiveModuleEnvEntry>,
}
static_assertions::assert_not_impl_any!(ActiveModuleEnv: Send, Sync);

impl ActiveModuleEnv {
    /// # Safety
    /// `env_priv` must remain valid and initialized for the entire lifetime of
    /// the returned `ActiveModuleEnv` guard so GC root collection can read it.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    unsafe fn push(env_priv: *mut emacs_env_private) -> Self {
        let heap_identity = neovm_value::tagged::gc::current_tagged_heap_identity().unwrap_or(0);
        Self {
            _scope: neovm_value::tls_scope::TlsStackScope::push(
                &ACTIVE_ENVS,
                ActiveModuleEnvEntry {
                    env_priv,
                    heap_identity,
                },
            ),
        }
    }
}

unsafe fn module_handle_nonlocal_exit(env: *mut emacs_env, flow: Flow) {
    let flow = flow.into_kind();
    unsafe {
        let priv_ = &mut *(*env).private_members;
        match flow {
            FlowKind::Signal(sig) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(sig.symbol);
                priv_.non_local_exit_data = sig
                    .raw_data
                    .unwrap_or_else(|| Value::list(sig.data.clone()));
            }
            FlowKind::Throw(thrown) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Throw;
                priv_.non_local_exit_symbol = thrown.tag;
                priv_.non_local_exit_data = thrown.value;
            }
            FlowKind::ThreadBlocked(_) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::symbol("error");
                priv_.non_local_exit_data =
                    Value::list(vec![Value::string("Thread blocked inside module call")]);
            }
            // The module ABI has no shutdown exit kind. Report it as a signal
            // so the module's own error path runs; the shutdown request is
            // already recorded, so the exit still happens once control returns
            // to the evaluator.
            FlowKind::Shutdown(request) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::symbol("kill-emacs");
                priv_.non_local_exit_data =
                    Value::list(vec![Value::fixnum(i64::from(request.exit_code))]);
            }
        }
    }
}

fn module_signal_or_throw(
    priv_: &emacs_env_private,
    symbol_mode: neovm_value::tagged::SymbolMode,
) -> Result<(), Flow> {
    match priv_.pending_non_local_exit {
        emacs_funcall_exit::Return => Ok(()),
        emacs_funcall_exit::Signal => {
            let sym_id = priv_
                .non_local_exit_symbol
                .as_bare_symbol_id()
                .unwrap_or_else(|| intern("error"));
            let sym_name = resolve_sym(sym_id);
            let raw_data = priv_.non_local_exit_data;
            Err(Flow::signal_boxed(Box::new(
                neovm_value::error::SignalData::new(
                    intern(sym_name),
                    neovm_value::value::list_to_vec(&raw_data).unwrap_or_else(|| vec![raw_data]),
                    Some(raw_data),
                    false,
                ),
            )))
        }
        // GNU emacs-module.c:1205 invokes Fthrow at final delivery; its
        // pending exit stores tag/value, not the earlier Lisp binding policy.
        emacs_funcall_exit::Throw => Err(Flow::throw_with_symbols(
            priv_.non_local_exit_symbol,
            priv_.non_local_exit_data,
            symbol_mode,
        )),
    }
}

// ============================================================================
// Panic containment at the module ABI boundary
// ============================================================================
//
// A Rust panic that reaches an `extern "C"` boundary aborts the process, so
// every `emacs_env` vtable primitive body runs under [`module_guard`] and the
// two trampolines catch around the module/Lisp execution. GNU's module layer
// reports every module-visible fault as a pending non-local exit; a caught
// panic becomes `Signal(error, "neomacs internal error: …")` the same way.
//
// Containment covers panics raised by HOST code. A panic raised inside a
// foreign Rust module's own code cannot reach these catches: such a module
// links its own libstd, and std deliberately aborts when a Rust panic from a
// different std instance hits `catch_unwind` (foreign-exception canary). C
// modules cannot panic at all. That matches GNU, where a module crashing
// internally crashes the editor.

/// Best-effort panic text shared with the evaluator's JIT shim boundary.
#[doc(hidden)]
pub use neovm_eval::emacs_core::eval::lifecycle::dynamic_module_panic_message as panic_message;

/// The `Flow` a contained panic turns into: a plain `error` signal (GNU's
/// module layer signals `error` with a descriptive string for internal module
/// faults), so `condition-case` handlers for `error` catch it.
fn module_panic_flow(payload: Box<dyn std::any::Any + Send>) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "neomacs internal error: {}",
            panic_message(&*payload)
        ))],
    )
}

/// True when a panic caught at the module ABI must be re-raised instead of
/// contained (probes the installed evaluator context, if any — see
/// `Context::module_panic_recovery_blocked`).
fn module_panic_recovery_blocked_via_ctx() -> bool {
    MODULE_CTX.with(|ctx_cell| {
        let ctx_ptr = ctx_cell.get();
        if ctx_ptr.is_null() {
            // No installed evaluator context (a primitive exercised outside
            // an active env extent). `gc_driver_active` is unreachable
            // without a Context, but GC lock poison is still observable via
            // the thread heap — probe that half, exactly like the JIT's
            // ctx-less shim arm (`contain_jit_shim_panic`).
            // SAFETY: read-only GC lock-poison probe; never collects, re-enters
            // `with_tagged_heap`, mints capability tokens, or mutates payloads.
            return unsafe { neovm_value::tagged::gc::with_tagged_heap(|h| h.gc_locks_poisoned()) };
        }
        // SAFETY: same shared-read-through-MODULE_CTX discipline as
        // `module_should_quit`; the probe only reads two flags.
        unsafe { (*ctx_ptr).module_panic_recovery_blocked() }
    })
}

/// Wraps every `extern "C"` vtable primitive body: a panic in the body is
/// caught, recorded as a pending `error` exit on `env` (first exit wins, so an
/// exit recorded before the panic is preserved), and the primitive returns its
/// existing error-convention `sentinel`. If the panic left GC state suspect it
/// is re-raised and aborts at the `extern "C"` shim — exactly the
/// pre-containment behavior for that class.
///
/// `AssertUnwindSafe`: the boundary's recovery contract (state restoration in
/// [`contain_lisp_panics`] / the trampolines, Drop-guarded host counters) is
/// what makes the crossing states coherent; a caught panic here never resumes
/// the broken computation.
/// # Safety
/// `env` must be null or a live `emacs_env` with valid `private_members`
/// exclusively accessed on the current thread so a caught panic can record a
/// pending signal.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe fn module_guard<R>(env: *mut emacs_env, sentinel: R, body: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => {
            if module_panic_recovery_blocked_via_ctx() {
                eprintln!(
                    "neomacs: refusing to contain a module-boundary panic \
                     (GC state suspect): {}",
                    panic_message(&*payload)
                );
                resume_unwind(payload);
            }
            let message = format!("neomacs internal error: {}", panic_message(&*payload));
            unsafe {
                set_pending_signal(env, "error", Value::list(vec![Value::string(&message)]));
            }
            sentinel
        }
    }
}

/// Every call from a module primitive into the evaluator goes through here:
/// snapshot the boundary state, run `f` under `catch_unwind`, and on a caught
/// panic restore the evaluator to the snapshot (the restoration an `Err(Flow)`
/// return performs frame-by-frame) and surface the panic as an `error` Flow —
/// indistinguishable at the ABI from a Lisp signal, so it rejoins the existing
/// `module_handle_nonlocal_exit` path.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn contain_lisp_panics<T>(
    ctx: &mut Context,
    f: impl FnOnce(&mut Context) -> Result<T, Flow>,
) -> Result<T, Flow> {
    let snap = ctx.module_boundary_snapshot();
    match catch_unwind(AssertUnwindSafe(|| f(&mut *ctx))) {
        Ok(result) => result,
        Err(payload) => {
            if ctx.module_panic_recovery_blocked() {
                eprintln!(
                    "neomacs: refusing to contain a module-boundary panic \
                     (GC state suspect): {}",
                    panic_message(&*payload)
                );
                resume_unwind(payload);
            }
            ctx.restore_module_boundary(&snap);
            Err(module_panic_flow(payload))
        }
    }
}

// ============================================================================
// Environment init
// ============================================================================

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe fn initialize_environment(env: *mut emacs_env, priv_: *mut emacs_env_private) {
    unsafe {
        (*priv_).pending_non_local_exit = emacs_funcall_exit::Return;
        (*priv_).non_local_exit_symbol = Value::NIL;
        (*priv_).non_local_exit_data = Value::NIL;
        (*priv_).storage.reset();

        let e = &mut *env;
        e.size = std::mem::size_of::<emacs_env>() as isize;
        e.private_members = priv_;

        e.make_global_ref = Some(module_make_global_ref);
        e.free_global_ref = Some(module_free_global_ref);
        e.non_local_exit_check = Some(module_non_local_exit_check);
        e.non_local_exit_clear = Some(module_non_local_exit_clear);
        e.non_local_exit_get = Some(module_non_local_exit_get);
        e.non_local_exit_signal = Some(module_non_local_exit_signal);
        e.non_local_exit_throw = Some(module_non_local_exit_throw);
        e.make_function = Some(module_make_function);
        e.funcall = Some(module_funcall);
        e.intern = Some(module_intern);
        e.type_of = Some(module_type_of);
        e.is_not_nil = Some(module_is_not_nil);
        e.eq = Some(module_eq);
        e.extract_integer = Some(module_extract_integer);
        e.make_integer = Some(module_make_integer);
        e.extract_float = Some(module_extract_float);
        e.make_float = Some(module_make_float);
        e.copy_string_contents = Some(module_copy_string_contents);
        e.make_string = Some(module_make_string);
        e.make_user_ptr = Some(module_make_user_ptr);
        e.get_user_ptr = Some(module_get_user_ptr);
        e.set_user_ptr = Some(module_set_user_ptr);
        e.get_user_finalizer = Some(module_get_user_finalizer);
        e.set_user_finalizer = Some(module_set_user_finalizer);
        e.vec_get = Some(module_vec_get);
        e.vec_set = Some(module_vec_set);
        e.vec_size = Some(module_vec_size);
        e.should_quit = Some(module_should_quit);
        e.process_input = Some(module_process_input);
        e.extract_time = Some(module_extract_time);
        e.make_time = Some(module_make_time);
        e.extract_big_integer = Some(module_extract_big_integer);
        e.make_big_integer = Some(module_make_big_integer);
        e.get_function_finalizer = Some(module_get_function_finalizer);
        e.set_function_finalizer = Some(module_set_function_finalizer);
        e.open_channel = Some(module_open_channel);
        e.make_interactive = Some(module_make_interactive);
        e.make_unibyte_string = Some(module_make_unibyte_string);
    }
}

// ============================================================================
// emacs_env function implementations
// ============================================================================

// --- Memory management ---

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe extern "C" fn module_make_global_ref(
    env: *mut emacs_env,
    value: emacs_value,
) -> emacs_value {
    let body = || {
        if value.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let lisp_val = unsafe { value_to_lisp(value) };
        // SAFETY: `env` is either null-checked above or the live `emacs_env` for this FFI call.
        if unsafe { check_pending_non_local_exit(env) } {
            return std::ptr::null_mut();
        }
        let heap_identity = neovm_value::tagged::gc::current_tagged_heap_identity().unwrap_or(0);
        with_global_refs(|slot| {
            let owner = slot.current().downgrade();
            let mut refs = slot.borrow_mut();
            for entry_opt in refs.iter_mut() {
                if let Some(entry) = entry_opt
                    && entry.heap_identity() == heap_identity
                    && entry.value().bits() == lisp_val.bits()
                {
                    (*(*entry.tag_ptr()).metadata()).refcount_add(1);
                    return entry.tag_ptr();
                }
            }
            let metadata = Box::into_raw(Box::new(GlobalRefMetadata::new(owner)));
            let mut tag_cell = emacs_value_tag::new(lisp_val);
            tag_cell.set_metadata(metadata);
            let tag = Box::into_raw(Box::new(tag_cell));
            // SAFETY: `tag` is `Box::into_raw` of a live `emacs_value_tag`.
            // The registry owns the entry, and `Drop` writes nil only into
            // that cell. It does not free it.
            let entry = GlobalRefEntry::new(lisp_val, heap_identity, tag);
            if let Some(pos) = refs.iter().position(|e| e.is_none()) {
                refs[pos] = Some(entry);
            } else {
                refs.push(Some(entry));
            }
            tag
        })
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe extern "C" fn module_free_global_ref(env: *mut emacs_env, global_value: emacs_value) {
    let body = || {
        if global_value.is_null() {
            return;
        }
        let metadata_ptr = (*global_value).metadata();
        if metadata_ptr.is_null() {
            return;
        }
        let metadata = &mut *metadata_ptr;
        metadata.refcount_add(-1);
        if metadata.refcount() > 0 {
            return;
        }
        // Resolve native ownership without reading the Lisp word, which has
        // been invalidated if its Context and heap have already been dropped.
        if let Some(owner) = metadata.owner().upgrade() {
            let mut refs = owner.borrow_mut();
            if let Some(entry) = refs.iter_mut().find(|entry| {
                entry
                    .as_ref()
                    .is_some_and(|entry| entry.tag_ptr() == global_value)
            }) {
                *entry = None;
            }
        }
        drop(Box::from_raw(metadata_ptr));
        drop(Box::from_raw(global_value));
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

// --- Non-local exits ---

unsafe extern "C" fn module_non_local_exit_check(env: *mut emacs_env) -> emacs_funcall_exit {
    let body = || {
        if env.is_null() {
            return emacs_funcall_exit::Return;
        }
        unsafe { (*env).private_members.as_ref() }
            .map(|p| p.pending_non_local_exit)
            .unwrap_or(emacs_funcall_exit::Return)
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, emacs_funcall_exit::Return, body) }
}

unsafe extern "C" fn module_non_local_exit_clear(env: *mut emacs_env) {
    let body = || {
        if env.is_null() {
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Return;
            priv_.non_local_exit_symbol = Value::NIL;
            priv_.non_local_exit_data = Value::NIL;
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

unsafe extern "C" fn module_non_local_exit_get(
    env: *mut emacs_env,
    symbol: *mut emacs_value,
    data: *mut emacs_value,
) -> emacs_funcall_exit {
    let body = || {
        if env.is_null() {
            return emacs_funcall_exit::Return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            let status = priv_.pending_non_local_exit;
            if status != emacs_funcall_exit::Return {
                if !symbol.is_null() {
                    // SAFETY: `env` was checked non-null above and owns initialized private handle storage.
                    *symbol = unsafe {
                        lisp_to_value_ignoring_pending_exit(env, priv_.non_local_exit_symbol)
                    };
                }
                if !data.is_null() {
                    // SAFETY: `env` was checked non-null above and owns initialized private handle storage.
                    *data = unsafe {
                        lisp_to_value_ignoring_pending_exit(env, priv_.non_local_exit_data)
                    };
                }
            }
            status
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, emacs_funcall_exit::Return, body) }
}

unsafe extern "C" fn module_non_local_exit_signal(
    env: *mut emacs_env,
    symbol: emacs_value,
    data: emacs_value,
) {
    let body = || {
        if env.is_null() {
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
            // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
            priv_.non_local_exit_symbol = unsafe { value_to_lisp(symbol) };
            // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
            priv_.non_local_exit_data = unsafe { value_to_lisp(data) };
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

unsafe extern "C" fn module_non_local_exit_throw(
    env: *mut emacs_env,
    tag: emacs_value,
    value: emacs_value,
) {
    let body = || {
        if env.is_null() {
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Throw;
            // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
            priv_.non_local_exit_symbol = unsafe { value_to_lisp(tag) };
            // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
            priv_.non_local_exit_data = unsafe { value_to_lisp(value) };
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

// --- Intern ---

unsafe extern "C" fn module_intern(
    env: *mut emacs_env,
    name: *const std::ffi::c_char,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if name.is_null() || !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        let cstr = unsafe { CStr::from_ptr(name) };
        let name = LispString::from_emacs_bytes(cstr.to_bytes().to_vec());
        let val = Value::from_sym_id(intern_lisp_string(&name));
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, val) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// --- Type conversion ---

unsafe extern "C" fn module_type_of(env: *mut emacs_env, arg: emacs_value) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        let type_name: &str = match val.kind() {
            neovm_value::value::ValueKind::Nil | neovm_value::value::ValueKind::T => "symbol",
            neovm_value::value::ValueKind::Fixnum(_) => "integer",
            neovm_value::value::ValueKind::Symbol(_) => "symbol",
            neovm_value::value::ValueKind::Cons => "cons",
            neovm_value::value::ValueKind::String => "string",
            neovm_value::value::ValueKind::Float => "float",
            neovm_value::value::ValueKind::Subr(_) => "primitive-function",
            neovm_value::value::ValueKind::Veclike(vt) => match vt {
                VecLikeType::Vector => "vector",
                VecLikeType::HashTable => "hash-table",
                VecLikeType::Obarray => "obarray",
                VecLikeType::Lambda | VecLikeType::Macro => "interpreted-function",
                VecLikeType::ByteCode => "byte-code-function",
                VecLikeType::Record => "record",
                VecLikeType::Font => "font-object",
                VecLikeType::WindowConfiguration => "window-configuration",
                VecLikeType::TsNode => "treesit-node",
                VecLikeType::Overlay => "overlay",
                VecLikeType::Marker => "marker",
                VecLikeType::Buffer => "buffer",
                VecLikeType::Window => "window",
                VecLikeType::Frame => "frame",
                VecLikeType::Timer => "timer",
                VecLikeType::Process => "process",
                VecLikeType::Terminal => "terminal",
                VecLikeType::Xwidget => "xwidget",
                VecLikeType::XwidgetView => "xwidget-view",
                VecLikeType::BoolVector => "bool-vector",
                VecLikeType::SurfaceHandle => "neomacs-surface",
                VecLikeType::VideoHandle => "neomacs-video",
                VecLikeType::Subr => "primitive-function",
                VecLikeType::Bignum => "bignum",
                VecLikeType::SymbolWithPos => "symbol-with-pos",
                VecLikeType::Finalizer => "finalizer",
                VecLikeType::Sqlite => "sqlite",
                VecLikeType::Thread => "thread",
                VecLikeType::Mutex => "mutex",
                VecLikeType::CondVar => "condition-variable",
                VecLikeType::UserPtr => "user-ptr",
                VecLikeType::ModuleFunction => "module-function",
                VecLikeType::CharTable => "char-table",
                VecLikeType::SubCharTable => "sub-char-table",
            },
            _ => "unknown",
        };
        let sym = Value::from_sym_id(intern(type_name));
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, sym) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

unsafe extern "C" fn module_is_not_nil(env: *mut emacs_env, arg: emacs_value) -> bool {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return false;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        !unsafe { value_to_lisp(arg) }.is_nil()
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, false, body) }
}

unsafe extern "C" fn module_eq(env: *mut emacs_env, a: emacs_value, b: emacs_value) -> bool {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || a.is_null() || b.is_null() {
            return false;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        (unsafe { value_to_lisp(a) }) == (unsafe { value_to_lisp(b) })
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, false, body) }
}

unsafe extern "C" fn module_extract_integer(env: *mut emacs_env, arg: emacs_value) -> i64 {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return 0;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(n) = val.as_fixnum() {
            return n;
        }
        if let Some(big) = val.as_bignum() {
            if let Ok(n) = i64::try_from(big) {
                return n;
            }
            unsafe {
                set_pending_signal(env, "overflow-error", Value::list(vec![val]));
            }
            return 0;
        }
        unsafe {
            set_pending_signal(
                env,
                "wrong-type-argument",
                Value::list(vec![Value::symbol("integerp"), val]),
            );
        }
        0
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, 0, body) }
}

unsafe extern "C" fn module_make_integer(env: *mut emacs_env, n: i64) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, Value::fixnum(n)) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

unsafe extern "C" fn module_extract_float(env: *mut emacs_env, arg: emacs_value) -> f64 {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return 0.0;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(f) = val.as_float() {
            return f;
        }
        unsafe {
            set_pending_signal(
                env,
                "wrong-type-argument",
                Value::list(vec![Value::symbol("floatp"), val]),
            );
        }
        0.0
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, 0.0, body) }
}

unsafe extern "C" fn module_make_float(env: *mut emacs_env, d: f64) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, Value::make_float(d)) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// --- String ---

unsafe extern "C" fn module_copy_string_contents(
    env: *mut emacs_env,
    value: emacs_value,
    buf: *mut std::ffi::c_char,
    len: *mut isize,
) -> bool {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || value.is_null() || len.is_null() {
            return false;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(value) };
        let bytes = match val.as_lisp_string() {
            Some(ls) => {
                if !ls.is_multibyte() && !ls.as_bytes().is_ascii() {
                    unsafe {
                        set_pending_signal(
                            env,
                            "wrong-type-argument",
                            Value::list(vec![Value::symbol("unicode-string-p"), val]),
                        );
                    }
                    return false;
                }
                match std::str::from_utf8(ls.as_bytes()) {
                    Ok(s) => s.as_bytes().to_vec(),
                    Err(_) => {
                        unsafe {
                            set_pending_signal(
                                env,
                                "wrong-type-argument",
                                Value::list(vec![Value::symbol("unicode-string-p"), val]),
                            );
                        }
                        return false;
                    }
                }
            }
            None => {
                unsafe {
                    set_pending_signal(
                        env,
                        "wrong-type-argument",
                        Value::list(vec![Value::symbol("stringp"), val]),
                    );
                }
                return false;
            }
        };
        let required_len = bytes.len() as isize + 1;

        if buf.is_null() {
            unsafe {
                *len = required_len;
            }
            return true;
        }

        let buf_len = unsafe { *len };
        if buf_len < required_len {
            unsafe {
                *len = required_len;
                set_pending_signal(
                    env,
                    "memory-buffer-too-small",
                    Value::list(vec![
                        Value::fixnum(buf_len as i64),
                        Value::fixnum(required_len as i64),
                    ]),
                );
            }
            return false;
        }
        if !bytes.is_empty() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr() as *const std::ffi::c_char,
                    buf,
                    bytes.len(),
                );
            }
        }
        unsafe {
            *buf.add(bytes.len()) = 0;
            *len = required_len;
        }
        true
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, false, body) }
}

/// Bytes of a module string argument.
///
/// # Safety
///
/// When `len > 0`, `ptr` must address `len` initialized bytes that stay valid
/// for `'a`. `len == 0` does not read `ptr` (GNU `emacs-module.c` returns the
/// empty string without consulting `str`). A null `ptr` with `len > 0` is
/// rejected and is not dereferenced.
unsafe fn module_input_bytes<'a>(ptr: *const u8, len: isize) -> Result<&'a [u8], ()> {
    if len < 0 {
        return Err(());
    }
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(());
    }
    // SAFETY: the caller promised `ptr` addresses `len` initialized bytes for `'a`.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
}

unsafe extern "C" fn module_make_string(
    env: *mut emacs_env,
    str: *const std::ffi::c_char,
    len: isize,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        // SAFETY: the module caller of `make_string` passes `len` initialized
        // bytes, or `len == 0`, matching `module_make_string` in emacs-module.c.
        let bytes = match unsafe { module_input_bytes(str as *const u8, len) } {
            Ok(bytes) => bytes,
            Err(()) => return std::ptr::null_mut(),
        };
        let s = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => {
                unsafe {
                    set_pending_signal(
                        env,
                        "wrong-type-argument",
                        Value::list(vec![
                            Value::symbol("utf-8-string-p"),
                            Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(
                                bytes.to_vec(),
                            )),
                        ]),
                    );
                }
                return std::ptr::null_mut();
            }
        };
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe {
            lisp_to_value(
                env,
                Value::heap_string(neovm_value::heap_types::LispString::from_utf8(s)),
            )
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

unsafe extern "C" fn module_make_unibyte_string(
    env: *mut emacs_env,
    str: *const std::ffi::c_char,
    len: isize,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        // SAFETY: the module caller of `make_unibyte_string` passes `len`
        // initialized bytes, or `len == 0`, matching emacs-module.c.
        let bytes = match unsafe { module_input_bytes(str as *const u8, len) } {
            Ok(bytes) => bytes,
            Err(()) => return std::ptr::null_mut(),
        };
        // SAFETY: the module ABI runs this FFI entry on the evaluator thread that
        // owns the installed heap; pure arena allocation (`alloc_string`) never
        // collects, sweeps, re-enters `with_tagged_heap`, mints capability tokens,
        // or mutates existing payloads.
        let val = unsafe {
            neovm_value::tagged::gc::with_tagged_heap(|h| {
                h.alloc_string(neovm_value::heap_types::LispString::from_unibyte(
                    bytes.to_vec(),
                ))
            })
        };
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, val) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// --- User pointer ---

unsafe extern "C" fn module_make_user_ptr(
    env: *mut emacs_env,
    fin: Option<unsafe extern "C" fn(*mut c_void)>,
    ptr: *mut c_void,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        let alloc = |h: &mut neovm_value::tagged::gc::TaggedHeap| {
            // SAFETY: the emacs-module ABI gives `fin` ownership of `ptr`.
            // This `unsafe extern "C"` entry is that contract.
            unsafe { h.alloc_user_ptr(ptr, fin) }
        };
        // SAFETY: the module ABI runs this FFI entry on the evaluator thread that
        // owns the installed heap; pure allocation (`alloc_user_ptr`) never
        // collects, sweeps, re-enters `with_tagged_heap`, mints capability tokens,
        // or mutates existing payloads.
        let val = unsafe { neovm_value::tagged::gc::with_tagged_heap(alloc) };
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, val) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

unsafe extern "C" fn module_get_user_ptr(env: *mut emacs_env, arg: emacs_value) -> *mut c_void {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        match val.as_user_ptr() {
            Some(up) => up.ptr,
            None => {
                if !env.is_null() {
                    unsafe {
                        let priv_ = &mut *(*env).private_members;
                        priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                        priv_.non_local_exit_symbol =
                            Value::from_sym_id(intern("wrong-type-argument"));
                        priv_.non_local_exit_data =
                            Value::list(vec![Value::symbol("user-ptrp"), val]);
                    }
                }
                std::ptr::null_mut()
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

unsafe extern "C" fn module_set_user_ptr(env: *mut emacs_env, arg: emacs_value, ptr: *mut c_void) {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(veclike_ptr) = val.as_veclike_ptr()
            && val.veclike_type() == Some(VecLikeType::UserPtr)
        {
            unsafe {
                let up = veclike_ptr as *mut UserPtrObj;
                (*up).ptr = ptr;
            }
            return;
        }
        if !env.is_null() {
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
                priv_.non_local_exit_data = Value::list(vec![Value::symbol("user-ptrp"), val]);
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

unsafe extern "C" fn module_get_user_finalizer(
    env: *mut emacs_env,
    uptr: emacs_value,
) -> Option<unsafe extern "C" fn(*mut c_void)> {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || uptr.is_null() {
            return None;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(uptr) };
        match val.as_user_ptr() {
            Some(up) => up.finalizer,
            None => {
                if !env.is_null() {
                    unsafe {
                        let priv_ = &mut *(*env).private_members;
                        priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                        priv_.non_local_exit_symbol =
                            Value::from_sym_id(intern("wrong-type-argument"));
                        priv_.non_local_exit_data =
                            Value::list(vec![Value::symbol("user-ptrp"), val]);
                    }
                }
                None
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, None, body) }
}

unsafe extern "C" fn module_set_user_finalizer(
    env: *mut emacs_env,
    arg: emacs_value,
    fin: Option<unsafe extern "C" fn(*mut c_void)>,
) {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(veclike_ptr) = val.as_veclike_ptr()
            && val.veclike_type() == Some(VecLikeType::UserPtr)
        {
            unsafe {
                let up = veclike_ptr as *mut UserPtrObj;
                (*up).finalizer = fin;
            }
            return;
        }
        if !env.is_null() {
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
                priv_.non_local_exit_data = Value::list(vec![Value::symbol("user-ptrp"), val]);
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

// --- Vector ---

unsafe extern "C" fn module_vec_get(
    env: *mut emacs_env,
    vector: emacs_value,
    index: isize,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || vector.is_null() || index < 0 {
            return std::ptr::null_mut();
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(vector) };
        if let Some(slice) = val.as_vector_data() {
            let idx = index as usize;
            if idx < slice.len() {
                // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
                return unsafe { lisp_to_value(env, slice.at(idx)) };
            }
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("args-out-of-range"));
                priv_.non_local_exit_data = Value::list(vec![val, Value::fixnum(index as i64)]);
            }
            return std::ptr::null_mut();
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
            priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
            priv_.non_local_exit_data = Value::list(vec![Value::symbol("vectorp"), val]);
        }
        std::ptr::null_mut()
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

unsafe extern "C" fn module_vec_set(
    env: *mut emacs_env,
    vector: emacs_value,
    index: isize,
    value: emacs_value,
) {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || vector.is_null() || index < 0 {
            return;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(vector) };
        if val.veclike_type() == Some(VecLikeType::Vector) {
            let idx = index as usize;
            // Route the slot store through the barriered chokepoint. Writing the
            // slot raw (as this did) skips the SATB pre-write log, so a concurrent
            // mark could sweep an object whose last live reference was the
            // overwritten slot — a use-after-free. `set_vector_slot` performs the
            // bounds check + `note_heap_slot_write` + atomic store, returning false
            // here only when `idx` is out of bounds.
            // SAFETY: handle was checked non-null above and belongs to the active module environment.
            if val.set_vector_slot(idx, unsafe { value_to_lisp(value) }) {
                return;
            }
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("args-out-of-range"));
                priv_.non_local_exit_data = Value::list(vec![val, Value::fixnum(index as i64)]);
            }
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
            priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
            priv_.non_local_exit_data = Value::list(vec![Value::symbol("vectorp"), val]);
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

unsafe extern "C" fn module_vec_size(env: *mut emacs_env, vector: emacs_value) -> isize {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || vector.is_null() {
            return 0;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(vector) };
        if let Some(slice) = val.as_vector_data() {
            return slice.len() as isize;
        }
        if !env.is_null() {
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
                priv_.non_local_exit_data = Value::list(vec![Value::symbol("vectorp"), val]);
            }
        }
        0
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, 0, body) }
}

// --- Quit / Input ---

unsafe extern "C" fn module_should_quit(env: *mut emacs_env) -> bool {
    let body = || {
        // SAFETY: `env` is either null-checked above or the live `emacs_env` for this FFI call.
        if env.is_null() || unsafe { check_pending_non_local_exit(env) } {
            return false;
        }
        MODULE_CTX.with(|ctx_cell| {
            let ctx_ptr = ctx_cell.get();
            if ctx_ptr.is_null() {
                return false;
            }
            unsafe {
                let ctx = &*ctx_ptr;
                ctx.quit_pending()
            }
        })
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, false, body) }
}

unsafe extern "C" fn module_process_input(env: *mut emacs_env) -> emacs_process_input_result {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return emacs_process_input_result::Quit;
        }
        MODULE_CTX.with(|ctx_cell| {
            let ctx_ptr = ctx_cell.get();
            if ctx_ptr.is_null() {
                return emacs_process_input_result::Continue;
            }
            // SAFETY: MODULE_CTX holds the live evaluator installed by the
            // enclosing trampoline (see ModuleContextGuard).
            let ctx = unsafe { &mut *ctx_ptr };
            match contain_lisp_panics(ctx, |ctx| ctx.maybe_quit()) {
                Ok(()) => emacs_process_input_result::Continue,
                Err(flow) => {
                    unsafe {
                        module_handle_nonlocal_exit(env, flow);
                    }
                    emacs_process_input_result::Quit
                }
            }
        })
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, emacs_process_input_result::Quit, body) }
}

// --- Time ---

unsafe extern "C" fn module_extract_time(env: *mut emacs_env, arg: emacs_value) -> emacs_time {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return emacs_time {
                tv_sec: 0,
                tv_nsec: 0,
            };
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(items) = val.as_vector_data()
            && items.len() >= 2
        {
            let sec_high = items.at(0).as_fixnum().unwrap_or(0);
            let sec_low = items.at(1).as_fixnum().unwrap_or(0);
            let micros = if items.len() >= 3 {
                items.at(2).as_fixnum().unwrap_or(0)
            } else {
                0
            };
            let picos = if items.len() >= 4 {
                items.at(3).as_fixnum().unwrap_or(0)
            } else {
                0
            };
            let seconds = (sec_high << 16) | (sec_low as u16 as i64);
            let nanoseconds = micros * 1000 + picos / 1000;
            return emacs_time {
                tv_sec: seconds as libc::time_t,
                tv_nsec: nanoseconds as libc::c_long,
            };
        }
        if let Some(n) = val.as_fixnum() {
            return emacs_time {
                tv_sec: n as libc::time_t,
                tv_nsec: 0,
            };
        }
        if !env.is_null() {
            // SAFETY: `env` was checked non-null above and its `private_members` is live for this call.
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
                priv_.non_local_exit_data = Value::list(vec![Value::symbol("timep"), val]);
            }
        }
        emacs_time {
            tv_sec: 0,
            tv_nsec: 0,
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe {
        module_guard(
            env,
            emacs_time {
                tv_sec: 0,
                tv_nsec: 0,
            },
            body,
        )
    }
}

unsafe extern "C" fn module_make_time(env: *mut emacs_env, time: emacs_time) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        // Route through the shared time-encoding seam (GNU
        // `timefns.c:timespec_to_lisp`, which `module_make_time` calls)
        // instead of inlining the list arithmetic. Two divergences retire
        // at once:
        //   * width -- `tv_nsec` is `libc::c_long` (`i64` on Linux/macOS,
        //     `i32` on Windows MSVC under LLP64); the seam takes fixed
        //     `i64`, so the platform-specific binding below is the single
        //     FFI/domain crossing rather than smearing conversions across four
        //     `Value::fixnum` calls that only compiled where `c_long == i64`.
        //   * shape -- the module `make_time` API contracts the canonical
        //     `(TICKS . HZ)` form (Hz = TIMESPEC_HZ = 1_000_000_000),
        //     which `LispTimeOutput::TicksHz` produces. The inlined code
        //     instead emitted the legacy `(HIGH LOW USEC PSEC)` list.
        // `make_lisp_time` reduces, sign-safely, to exactly
        // `tv_sec*1e9 + tv_nsec` ticks -- byte-identical to `timespec_ticks`.
        #[cfg(target_os = "windows")]
        let nanoseconds = i64::from(time.tv_nsec);
        #[cfg(not(target_os = "windows"))]
        let nanoseconds = time.tv_nsec;
        let lisp_time = make_lisp_time(time.tv_sec, nanoseconds, LispTimeOutput::TicksHz);
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, lisp_time) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// --- Big integer ---

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe extern "C" fn module_extract_big_integer(
    env: *mut emacs_env,
    arg: emacs_value,
    sign: *mut std::ffi::c_int,
    count: *mut isize,
    magnitude: *mut emacs_limb_t,
) -> bool {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return false;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(n) = val.as_fixnum() {
            unsafe {
                if !sign.is_null() {
                    *sign = (n > 0) as std::ffi::c_int - (n < 0) as std::ffi::c_int;
                }
            }
            if n == 0 || count.is_null() {
                return true;
            }
            let abs_val = n.unsigned_abs();
            if magnitude.is_null() {
                unsafe {
                    *count = 1;
                }
                return true;
            }
            if unsafe { *count } < 1 {
                let actual = unsafe { *count };
                unsafe {
                    *count = 1;
                    set_pending_signal(
                        env,
                        "memory-buffer-too-small",
                        Value::list(vec![Value::fixnum(actual as i64), Value::fixnum(1)]),
                    );
                }
                return false;
            }
            unsafe {
                *magnitude = abs_val;
                *count = 1;
            }
            return true;
        }
        if val.veclike_type() == Some(VecLikeType::Bignum) {
            let b = val.as_bignum().unwrap();
            let is_neg = *b < 0;
            let abs_b = if is_neg { -b.clone() } else { b.clone() };
            unsafe {
                if !sign.is_null() {
                    *sign = if *b > 0 {
                        1
                    } else if is_neg {
                        -1
                    } else {
                        0
                    };
                }
            }
            if count.is_null() {
                return true;
            }
            let limbs = abs_b.to_twos_complement_limbs_asc();
            let num_limbs = limbs.len();
            if magnitude.is_null() {
                unsafe {
                    *count = num_limbs as isize;
                }
                return true;
            }
            if unsafe { *count } < num_limbs as isize {
                let actual = unsafe { *count };
                unsafe {
                    *count = num_limbs as isize;
                    set_pending_signal(
                        env,
                        "memory-buffer-too-small",
                        Value::list(vec![
                            Value::fixnum(actual as i64),
                            Value::fixnum(num_limbs as i64),
                        ]),
                    );
                }
                return false;
            }
            unsafe {
                *count = num_limbs as isize;
                for (i, &limb) in limbs.iter().enumerate() {
                    *magnitude.add(i) = limb;
                }
            }
            return true;
        }
        if !env.is_null() {
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
                priv_.non_local_exit_data = Value::list(vec![Value::symbol("integerp"), val]);
            }
        }
        false
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, false, body) }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe extern "C" fn module_make_big_integer(
    env: *mut emacs_env,
    sign: std::ffi::c_int,
    count: isize,
    magnitude: *const emacs_limb_t,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        if sign == 0 {
            // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
            return unsafe { lisp_to_value(env, Value::fixnum(0)) };
        }
        if count == 0 {
            // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
            return unsafe { lisp_to_value(env, Value::fixnum(0)) };
        }
        if magnitude.is_null() || count < 0 {
            return std::ptr::null_mut();
        }
        let limbs = unsafe { std::slice::from_raw_parts(magnitude, count as usize) };
        if count <= 1 {
            let single = unsafe { *magnitude };
            if single <= i64::MAX as u64 {
                let v = if sign >= 0 {
                    single as i64
                } else {
                    -(single as i64)
                };
                // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
                return unsafe { lisp_to_value(env, Value::make_int(v)) };
            }
        }
        let b = Integer::from_twos_complement_limbs_asc(limbs);
        let val = Value::make_integer(if sign >= 0 { b } else { -b });
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, val) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// --- Function finalizer ---

unsafe extern "C" fn module_get_function_finalizer(
    env: *mut emacs_env,
    arg: emacs_value,
) -> Option<unsafe extern "C" fn(*mut c_void)> {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return None;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        match val.as_module_function() {
            Some(mf) => mf.finalizer,
            None => {
                if !env.is_null() {
                    unsafe {
                        let priv_ = &mut *(*env).private_members;
                        priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                        priv_.non_local_exit_symbol =
                            Value::from_sym_id(intern("wrong-type-argument"));
                        priv_.non_local_exit_data =
                            Value::list(vec![Value::symbol("module-function-p"), val]);
                    }
                }
                None
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, None, body) }
}

unsafe extern "C" fn module_set_function_finalizer(
    env: *mut emacs_env,
    arg: emacs_value,
    fin: Option<unsafe extern "C" fn(*mut c_void)>,
) {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || arg.is_null() {
            return;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let val = unsafe { value_to_lisp(arg) };
        if let Some(veclike_ptr) = val.as_veclike_ptr()
            && val.veclike_type() == Some(VecLikeType::ModuleFunction)
        {
            unsafe {
                let mf = veclike_ptr as *mut ModuleFunctionObj;
                (*mf).finalizer = fin;
            }
            return;
        }
        if !env.is_null() {
            unsafe {
                let priv_ = &mut *(*env).private_members;
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
                priv_.non_local_exit_data =
                    Value::list(vec![Value::symbol("module-function-p"), val]);
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

// --- Pipe channel ---

unsafe extern "C" fn module_open_channel(
    env: *mut emacs_env,
    pipe_process: emacs_value,
) -> std::ffi::c_int {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || pipe_process.is_null() {
            return -1;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let process = unsafe { value_to_lisp(pipe_process) };
        MODULE_CTX.with(|ctx_cell| {
            let ctx_ptr = ctx_cell.get();
            if ctx_ptr.is_null() {
                unsafe {
                    set_pending_signal(
                        env,
                        "error",
                        Value::list(vec![Value::string(
                            "no evaluator context available for module open_channel",
                        )]),
                    );
                }
                return -1;
            }
            // SAFETY: MODULE_CTX holds the live evaluator installed by the
            // enclosing trampoline (see ModuleContextGuard).
            let ctx = unsafe { &mut *ctx_ptr };
            match contain_lisp_panics(ctx, |ctx| ctx.open_channel_for_module(process)) {
                Ok(fd) => fd,
                Err(flow) => {
                    unsafe {
                        module_handle_nonlocal_exit(env, flow);
                    }
                    -1
                }
            }
        })
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, -1, body) }
}

// --- Interactive ---

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe extern "C" fn module_make_interactive(
    env: *mut emacs_env,
    function: emacs_value,
    spec: emacs_value,
) {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || function.is_null() {
            return;
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let func_val = unsafe { value_to_lisp(function) };
        // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
        let spec_val = unsafe { value_to_lisp(spec) };
        if let Some(veclike_ptr) = func_val.as_veclike_ptr()
            && func_val.veclike_type() == Some(VecLikeType::ModuleFunction)
        {
            // SATB deletion barrier BEFORE clobbering the live, GC-traced
            // `interactive_form` slot: log the pre-overwrite children so a value
            // reachable only through the old form is retained if a concurrent
            // mark is in flight. Owner-driven; a no-op unless a mark is active.
            // Without it, a second make-interactive overwriting a still-reachable
            // form mid-mark would drop it. See CONCURRENT_GC.md "Insertion
            // coverage".
            note_heap_write(func_val, HeapWriteKind::ModuleFunction);
            unsafe {
                let mf = veclike_ptr as *mut ModuleFunctionObj;
                (*mf).interactive_form = if spec_val.is_nil() {
                    Value::list(vec![Value::symbol("interactive")])
                } else {
                    Value::list(vec![Value::symbol("interactive"), spec_val])
                };
            }
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
            priv_.non_local_exit_symbol = Value::from_sym_id(intern("wrong-type-argument"));
            priv_.non_local_exit_data =
                Value::list(vec![Value::symbol("module-function-p"), func_val]);
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, (), body) }
}

// ============================================================================
// make_function
// ============================================================================

#[cfg_attr(feature = "test-support", visibility::make(pub))]
unsafe extern "C" fn module_make_function(
    env: *mut emacs_env,
    min_arity: isize,
    max_arity: isize,
    subr: emacs_function,
    docstring: *const std::ffi::c_char,
    data: *mut c_void,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } {
            return std::ptr::null_mut();
        }
        let most_positive_fixnum = Value::MOST_POSITIVE_FIXNUM as isize;
        let valid_arity = min_arity >= 0
            && if max_arity < 0 {
                min_arity <= most_positive_fixnum && max_arity == emacs_variadic_function
            } else {
                min_arity <= max_arity && max_arity <= most_positive_fixnum
            };
        if !valid_arity {
            unsafe {
                set_pending_signal(
                    env,
                    "invalid-arity",
                    Value::list(vec![
                        Value::fixnum(min_arity as i64),
                        Value::fixnum(max_arity as i64),
                    ]),
                );
            }
            return std::ptr::null_mut();
        }
        let doc_val = if docstring.is_null() {
            Value::NIL
        } else {
            let cstr = unsafe { CStr::from_ptr(docstring) };
            match cstr.to_str() {
                Ok(s) => Value::heap_string(neovm_value::heap_types::LispString::from_utf8(s)),
                Err(_) => {
                    unsafe {
                        set_pending_signal(
                            env,
                            "wrong-type-argument",
                            Value::list(vec![
                                Value::symbol("utf-8-string-p"),
                                Value::heap_string(
                                    neovm_value::heap_types::LispString::from_unibyte(
                                        cstr.to_bytes().to_vec(),
                                    ),
                                ),
                            ]),
                        );
                    }
                    return std::ptr::null_mut();
                }
            }
        };
        let alloc = |h: &mut neovm_value::tagged::gc::TaggedHeap| {
            // SAFETY: the emacs-module ABI passes `subr` as a live `emacs_function`
            // and `data` that outlives every later invocation.
            unsafe {
                h.alloc_module_function(
                    min_arity,
                    max_arity,
                    subr as *const c_void,
                    data,
                    doc_val,
                    Value::NIL,
                )
            }
        };
        // SAFETY: the module ABI runs this FFI entry on the evaluator thread that
        // owns the installed heap; pure allocation (`alloc_module_function`) never
        // collects, sweeps, re-enters `with_tagged_heap`, mints capability tokens,
        // or mutates existing payloads.
        let val = unsafe { neovm_value::tagged::gc::with_tagged_heap(alloc) };
        // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
        unsafe { lisp_to_value(env, val) }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// ============================================================================
// funcall: C → Lisp bridging
// ============================================================================

unsafe extern "C" fn module_funcall(
    env: *mut emacs_env,
    func: emacs_value,
    nargs: isize,
    args: *mut emacs_value,
) -> emacs_value {
    let body = || {
        // SAFETY: `env` is either null or the live `emacs_env` pointer for this FFI call.
        if !unsafe { module_function_begin(env) } || func.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: handle was checked non-null above and belongs to the active module environment.
        let func_val = unsafe { value_to_lisp(func) };
        let mut lisp_args = Vec::with_capacity(nargs as usize);
        for i in 0..nargs as usize {
            let arg = unsafe { *args.add(i) };
            // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
            lisp_args.push(unsafe { value_to_lisp(arg) });
        }

        let result = MODULE_CTX.with(|ctx_cell| {
            let ctx_ptr = ctx_cell.get();
            if ctx_ptr.is_null() {
                return Err(signal(
                    "error",
                    vec![Value::string(
                        "no evaluator context available for module funcall",
                    )],
                ));
            }
            // SAFETY: MODULE_CTX holds the live evaluator installed by the
            // enclosing trampoline (see ModuleContextGuard).
            let ctx = unsafe { &mut *ctx_ptr };
            contain_lisp_panics(ctx, |ctx| ctx.funcall_general_untraced(func_val, lisp_args))
        });

        match result {
            // SAFETY: `module_function_begin(env)` confirmed `env` is live with valid handle storage.
            Ok(ret_val) => unsafe { lisp_to_value(env, ret_val) },
            Err(flow) => {
                unsafe {
                    module_handle_nonlocal_exit(env, flow);
                }
                std::ptr::null_mut()
            }
        }
    };
    // SAFETY: `env` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(env, std::ptr::null_mut(), body) }
}

// ============================================================================
// Thread-local context pointer for FFI callbacks
// ============================================================================

thread_local! {
    static MODULE_CTX: std::cell::Cell<*mut Context> =
        const { std::cell::Cell::new(std::ptr::null_mut()) };
}

/// Read-only observer of `MODULE_CTX` for cross-crate tests; the underlying
/// cell remains module-private so external code cannot forge a context pointer.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub fn current_module_context_for_test() -> *mut Context {
    MODULE_CTX.with(|slot| slot.get())
}

/// Self-contained verification helper for cross-crate tests: proves
/// `with_module_context` restores a *distinct* prior `MODULE_CTX` address on
/// both normal return and panic unwind while keeping `MODULE_CTX` private and
/// always restoring the slot to null before returning.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub fn verify_with_module_context_restores_distinct_previous_for_test(ctx: &mut Context) {
    let ctx_ptr: *mut Context = std::ptr::from_mut(ctx);
    // Never dereferenced: only distinguishes the saved outer slot address
    // from `ctx_ptr` and from null.
    let outer_sentinel = std::ptr::without_provenance_mut::<Context>(0x1000);
    assert_ne!(ctx_ptr, outer_sentinel);
    let current = || MODULE_CTX.with(|slot| slot.get());

    assert!(current().is_null());
    {
        let _outer = neovm_value::tls_scope::TlsScope::new(&MODULE_CTX, outer_sentinel);
        assert_eq!(current(), outer_sentinel);

        with_module_context(ctx, || {
            assert_eq!(current(), ctx_ptr);
        });
        assert_eq!(
            current(),
            outer_sentinel,
            "normal return must restore the distinct outer context pointer"
        );

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_module_context(ctx, || {
                assert_eq!(current(), ctx_ptr);
                panic!("boom inside with_module_context");
            });
        }));
        assert!(panicked.is_err());
        assert_eq!(
            current(),
            outer_sentinel,
            "panic unwind must restore the distinct outer context pointer"
        );
    }
    assert!(current().is_null());
}

/// Private lifetime-branded RAII installer for `MODULE_CTX`: borrows
/// `&'a mut Context` exclusively, saves the previous pointer on construction,
/// and restores it on `Drop` (precedent: [`ActiveModuleEnv`]).
///
/// Restoring — rather than clearing to NULL — is what makes nested
/// module→elisp→module calls work: when the inner `apply_module_function`
/// returns, the outer call's context must come back, or the outer module
/// function's next `env->funcall` finds no evaluator context. `Drop` also
/// keeps the teardown coherent once panics become catchable.
///
/// Kept private behind [`with_module_context`] so callers can neither forge a
/// raw `*mut Context`, alias `&mut Context` during the scope, nor leak the
/// guard via `std::mem::forget` to leave a dangling `MODULE_CTX`.
#[must_use = "the thread-local extent ends when this guard drops"]
#[derive(Debug)]
struct ModuleContextGuard<'a> {
    _scope: neovm_value::tls_scope::TlsScope<*mut Context, std::cell::Cell<*mut Context>>,
    _ctx: std::marker::PhantomData<&'a mut Context>,
}
static_assertions::assert_not_impl_any!(ModuleContextGuard<'_>: Send, Sync);

impl<'a> ModuleContextGuard<'a> {
    fn install(ctx: &'a mut Context) -> Self {
        let ptr: *mut Context = std::ptr::from_mut(ctx);
        Self {
            _scope: neovm_value::tls_scope::TlsScope::new(&MODULE_CTX, ptr),
            _ctx: std::marker::PhantomData,
        }
    }
}

/// Run `body` with `ctx` installed in `MODULE_CTX` for the dynamic extent of
/// the closure, restoring the previous pointer on normal return or unwind.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn with_module_context<R>(ctx: &mut Context, body: impl FnOnce() -> R) -> R {
    let _guard = ModuleContextGuard::install(ctx);
    body()
}

// ============================================================================
// emacs_runtime get_environment
// ============================================================================

unsafe extern "C" fn module_get_environment(rt: *mut emacs_runtime) -> *mut emacs_env {
    // No env exists yet to record a pending exit on; a caught panic here just
    // returns NULL (which module init code must treat as failure anyway).
    let body = || {
        if rt.is_null() {
            return std::ptr::null_mut();
        }
        unsafe {
            let priv_ = &*(*rt).private_members;
            priv_.env
        }
    };
    // SAFETY: `std::ptr::null_mut()` is either null or the live `emacs_env` vtable pointer
    // supplied by the dynamic-module caller for the duration of this FFI call.
    unsafe { module_guard(std::ptr::null_mut(), std::ptr::null_mut(), body) }
}

// ============================================================================
// module-load entry point
// ============================================================================

pub fn load_module(ctx: &mut Context, path: std::path::PathBuf) -> EvalResult {
    // Load the shared library from the byte-faithful path (eight-bit-safe on
    // Unix); `path_str` is a display form used only for error text and the
    // already-loaded registry key (module paths are ASCII in practice).
    let path_str = path.display().to_string();
    let lib = ModuleLibrary::open(&path).map_err(|ModuleOpenError::Loader(e)| {
        signal(
            LispCondition::ModuleOpenFailed,
            vec![Value::string(&path_str), Value::string(e.to_string())],
        )
    })?;

    {
        // SAFETY: this presence-only probe never dereferences the symbol;
        // ModuleLibrary retains its mapping across lookup and all later calls.
        let has_gpl: Result<libloading::Symbol<*const ()>, _> =
            unsafe { lib.library.get(b"plugin_is_GPL_compatible") };
        if has_gpl.is_err() {
            return Err(signal(
                LispCondition::ModuleNotGplCompatible,
                vec![Value::string(&path_str)],
            ));
        }
    }

    type EmacsInitFn = unsafe extern "C" fn(runtime: *mut emacs_runtime) -> std::ffi::c_int;
    // SAFETY: GNU-compatible modules export this exact C initialization ABI;
    // the process-lifetime library keeps its entry point mapped.
    let init_fn: libloading::Symbol<EmacsInitFn> = unsafe { lib.library.get(b"emacs_module_init") }
        .map_err(|_| {
            signal(
                LispCondition::MissingModuleInitFunction,
                vec![Value::string(&path_str)],
            )
        })?;
    let init_fn_ptr: EmacsInitFn = *init_fn;

    let mut env_priv = emacs_env_private::new_boxed();
    let env_priv_ptr: *mut emacs_env_private = &mut *env_priv;

    let mut env_box = Box::new(emacs_env {
        size: 0,
        private_members: std::ptr::null_mut(),
        make_global_ref: None,
        free_global_ref: None,
        non_local_exit_check: None,
        non_local_exit_clear: None,
        non_local_exit_get: None,
        non_local_exit_signal: None,
        non_local_exit_throw: None,
        make_function: None,
        funcall: None,
        intern: None,
        type_of: None,
        is_not_nil: None,
        eq: None,
        extract_integer: None,
        make_integer: None,
        extract_float: None,
        make_float: None,
        copy_string_contents: None,
        make_string: None,
        make_unibyte_string: None,
        make_user_ptr: None,
        get_user_ptr: None,
        set_user_ptr: None,
        get_user_finalizer: None,
        set_user_finalizer: None,
        vec_get: None,
        vec_set: None,
        vec_size: None,
        should_quit: None,
        process_input: None,
        extract_time: None,
        make_time: None,
        extract_big_integer: None,
        make_big_integer: None,
        get_function_finalizer: None,
        set_function_finalizer: None,
        open_channel: None,
        make_interactive: None,
    });

    let env_ptr: *mut emacs_env = &mut *env_box;
    env_box.private_members = env_priv_ptr;
    // SAFETY: both pointers refer to owned initialized boxes, and the
    // environment table is completed before any module receives it.
    unsafe {
        initialize_environment(env_ptr, env_priv_ptr);
    }

    let rt_priv = Box::new(emacs_runtime_private { env: env_ptr });
    let mut rt = Box::new(emacs_runtime {
        size: std::mem::size_of::<emacs_runtime>() as isize,
        private_members: Box::into_raw(rt_priv),
        get_environment: Some(module_get_environment),
    });

    // SAFETY: the private env box outlives `active_env`, which drops before `env_priv` is finalized.
    let active_env = unsafe { ActiveModuleEnv::push(env_priv_ptr) };
    let init_code = with_module_context(ctx, || {
        // SAFETY: the export has the Emacs initialization ABI, its library is
        // retained, and rt plus its environment remain owned across the call.
        unsafe { init_fn_ptr(&mut *rt as *mut emacs_runtime) }
    });

    ctx.maybe_quit()?;
    if init_code != 0 {
        return Err(signal(
            LispCondition::ModuleInitFailed,
            vec![Value::string(&path_str), Value::fixnum(init_code as i64)],
        ));
    }

    // SAFETY: env_priv still owns this initialized environment, and the
    // initializer has returned before its pending Lisp exit is inspected.
    let env_priv_ref = unsafe { &*env_priv_ptr };
    module_signal_or_throw(env_priv_ref, ctx.symbol_mode())?;
    drop(active_env);

    finalize_storage(&mut env_priv.storage);
    // SAFETY: `private_members` came from `Box::into_raw` above and nothing
    // else frees it. Init has returned, so no module code reads the runtime.
    drop(unsafe { Box::from_raw(rt.private_members) });
    drop(rt);
    drop(env_box);
    drop(env_priv);

    let loaded = LoadedModule { library: lib };

    // Heal poison rather than unwrap: the registry is a plain map with no
    // invariant spanning the lock (an interrupted insert either happened or
    // didn't), so a panic contained elsewhere must not wedge `module-load`.
    LOADED_MODULES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(path_str, loaded);

    Ok(Value::T)
}

// ============================================================================
// funcall_module — Lisp → C module function dispatch
// ============================================================================

pub fn apply_module_function(ctx: &mut Context, func: Value, args: Vec<Value>) -> EvalResult {
    let mf = func
        .as_module_function()
        .ok_or_else(|| signal(LispCondition::InvalidFunction, vec![func]))?;

    let nargs = args.len() as isize;
    if nargs < mf.min_arity {
        return Err(signal(LispCondition::WrongNumberOfArguments, vec![func]));
    }
    if mf.max_arity >= 0 && nargs > mf.max_arity {
        return Err(signal(LispCondition::WrongNumberOfArguments, vec![func]));
    }

    // SAFETY: module_make_function stores an emacs_function at this address.
    // ModuleLibrary::open keeps native code mapped for the process lifetime,
    // including failed initialization; host-provided callbacks are static.
    let subr_fn: emacs_function = unsafe { std::mem::transmute(mf.subr) };
    let data = mf.data;

    let mut env_priv = emacs_env_private::new_boxed();

    let mut env = Box::new(emacs_env {
        size: 0,
        private_members: std::ptr::null_mut(),
        make_global_ref: None,
        free_global_ref: None,
        non_local_exit_check: None,
        non_local_exit_clear: None,
        non_local_exit_get: None,
        non_local_exit_signal: None,
        non_local_exit_throw: None,
        make_function: None,
        funcall: None,
        intern: None,
        type_of: None,
        is_not_nil: None,
        eq: None,
        extract_integer: None,
        make_integer: None,
        extract_float: None,
        make_float: None,
        copy_string_contents: None,
        make_string: None,
        make_unibyte_string: None,
        make_user_ptr: None,
        get_user_ptr: None,
        set_user_ptr: None,
        get_user_finalizer: None,
        set_user_finalizer: None,
        vec_get: None,
        vec_set: None,
        vec_size: None,
        should_quit: None,
        process_input: None,
        extract_time: None,
        make_time: None,
        extract_big_integer: None,
        make_big_integer: None,
        get_function_finalizer: None,
        set_function_finalizer: None,
        open_channel: None,
        make_interactive: None,
    });

    let env_ptr: *mut emacs_env = &mut *env;
    let priv_ptr: *mut emacs_env_private = &mut *env_priv;
    unsafe {
        env.private_members = priv_ptr;
        initialize_environment(env_ptr, priv_ptr);
    }

    let mut emacs_args: Vec<emacs_value> = Vec::with_capacity(args.len());
    for arg in &args {
        // SAFETY: `env_ptr` points to the initialized `emacs_env` owned by this call.
        emacs_args.push(unsafe { lisp_to_value(env_ptr, *arg) });
    }

    // Call the module function through a "C-unwind" view of the stored
    // pointer: ABI-identical to "C", but a panic propagating out of the callee
    // becomes defined behavior (it reaches the catch below) instead of UB at
    // the call site. Real C modules never unwind; host-defined module
    // functions (and any future same-std ones) may.
    let subr_fn_unwind: unsafe extern "C-unwind" fn(
        env: *mut emacs_env,
        nargs: isize,
        args: *mut emacs_value,
        data: *mut c_void,
    ) -> emacs_value = unsafe { std::mem::transmute(subr_fn) };

    let snap = ctx.module_boundary_snapshot();
    // SAFETY: the private env box outlives `active_env`, which drops before `env_priv` is finalized.
    let active_env = unsafe { ActiveModuleEnv::push(priv_ptr) };
    let args_ptr = emacs_args.as_mut_ptr();
    let call = with_module_context(ctx, || {
        // SAFETY: `subr_fn_unwind` has the verified module-function C ABI from
        // a permanently mapped library, `env_ptr` and `args_ptr[..nargs]` stay
        // live for this call, and `with_module_context` / `active_env` own the
        // thread-local context and local-value roots.
        catch_unwind(AssertUnwindSafe(|| unsafe {
            subr_fn_unwind(env_ptr, nargs, args_ptr, data)
        }))
    });

    let (result, panic_flow) = match call {
        Ok(result) => (result, None),
        Err(payload) => {
            if ctx.module_panic_recovery_blocked() {
                eprintln!(
                    "neomacs: refusing to contain a module-boundary panic \
                     (GC state suspect): {}",
                    panic_message(&*payload)
                );
                // The unwind runs the ActiveModuleEnv/env Drops; the process
                // aborts once the panic reaches an `extern "C"` frame.
                resume_unwind(payload);
            }
            ctx.restore_module_boundary(&snap);
            (std::ptr::null_mut(), Some(module_panic_flow(payload)))
        }
    };

    if panic_flow.is_none()
        && let Err(flow) = ctx.maybe_quit()
    {
        unsafe {
            module_handle_nonlocal_exit(env_ptr, flow);
        }
    }

    let priv_ref = unsafe { &*priv_ptr };
    // SAFETY: `value_to_lisp` handles null as `NIL` and dereferences only live module handles.
    let ret = unsafe { value_to_lisp(result) };
    // A contained panic outranks any exit the module recorded before dying:
    // the env's pending state was part of the computation the panic tore down.
    let exit = match panic_flow {
        Some(flow) => Err(flow),
        None => module_signal_or_throw(priv_ref, ctx.symbol_mode()),
    };
    drop(active_env);

    finalize_storage(&mut env_priv.storage);
    drop(env);
    drop(env_priv);

    exit?;
    Ok(ret)
}

/// Carrier park: clear the module-environment stack and the raw Context
/// pointer. `GLOBAL_REFS` is reset by `reset_dynamic_module_registry`.
#[doc(hidden)]
pub fn reset_carrier_tls() {
    ACTIVE_ENVS.with(|stack| stack.borrow_mut().clear());
    MODULE_CTX.with(|slot| slot.set(std::ptr::null_mut()));
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix(h, ACTIVE_ENVS.with(|slot| slot.borrow().len() as u64));
    neovm_value::tls_fingerprint::mix(
        h,
        dynamic_module_registry_heap_identity().unwrap_or(0) as u64,
    );
    neovm_value::tls_fingerprint::mix(h, MODULE_CTX.with(|slot| slot.get() as usize as u64));
}

#[cfg(test)]
mod module_string_tests {
    use super::module_input_bytes;

    #[test]
    fn module_make_string_null_zero_is_empty_without_reading_the_pointer() {
        // SAFETY: length 0 does not read the pointer.
        let bytes = unsafe { module_input_bytes(std::ptr::null(), 0) }.expect("empty string");
        assert!(bytes.is_empty());
    }

    #[test]
    fn module_make_string_null_nonzero_is_rejected() {
        // SAFETY: a null pointer with a positive length is rejected before any read.
        assert!(unsafe { module_input_bytes(std::ptr::null(), 4) }.is_err());
    }
}

#[cfg(test)]
mod value_storage_tests {
    use super::{
        ActiveModuleEnv, VALUE_FRAME_SIZE, allocate_emacs_value, collect_dynamic_module_gc_roots,
        emacs_env_private, emacs_value_storage, finalize_storage, value_to_lisp,
    };
    use neovm_value::value::Value;

    #[test]
    fn emacs_value_storage_allocates_immediately_after_new() {
        let mut storage = emacs_value_storage::new();
        let handle = allocate_emacs_value(&mut storage, Value::fixnum(42));
        // SAFETY: `handle` was allocated from `storage` above and remains live.
        assert_eq!(unsafe { value_to_lisp(handle) }, Value::fixnum(42));
        finalize_storage(&mut storage);
    }

    #[test]
    fn emacs_value_storage_move_after_initial_alloc_and_overflow_finalize_is_sound() {
        let mut storage = emacs_value_storage::new();
        let pre_move = 5usize;
        for i in 0..pre_move {
            let _ = allocate_emacs_value(&mut storage, Value::fixnum((i + 1) as i64));
        }
        // Move storage AFTER allocating into `initial`; no self-pointer may
        // dangle and previously stored values in `initial` stay intact.
        let mut env_priv = Box::new(emacs_env_private {
            pending_non_local_exit: super::emacs_funcall_exit::Return,
            non_local_exit_symbol: Value::NIL,
            non_local_exit_data: Value::NIL,
            storage,
        });
        let post_move = VALUE_FRAME_SIZE * 2 + 17;
        let mut handles = Vec::with_capacity(post_move);
        for i in 0..post_move {
            handles.push(allocate_emacs_value(
                &mut env_priv.storage,
                Value::fixnum((pre_move + i + 1) as i64),
            ));
        }
        for (i, &h) in handles.iter().enumerate() {
            // SAFETY: `env_priv` has not moved since `h` was allocated and has not been finalized.
            assert_eq!(
                unsafe { value_to_lisp(h) },
                Value::fixnum((pre_move + i + 1) as i64)
            );
        }
        let priv_ptr: *mut emacs_env_private = &mut *env_priv;
        // SAFETY: `env_priv` stays boxed and live until after `_active` drops.
        let active = unsafe { ActiveModuleEnv::push(priv_ptr) };
        let heap_id = neovm_value::tagged::gc::current_tagged_heap_identity().unwrap_or(0);
        let mut roots = Vec::new();
        collect_dynamic_module_gc_roots(&mut roots, heap_id);
        drop(active);
        let mut seen: Vec<i64> = roots
            .iter()
            .filter_map(|v| v.as_fixnum())
            .filter(|&n| n > 0)
            .collect();
        seen.sort_unstable();
        let expected: Vec<i64> = (1..=((pre_move + post_move) as i64)).collect();
        assert_eq!(seen, expected);

        finalize_storage(&mut env_priv.storage);
        // Repeated finalization and Drop are idempotent.
        finalize_storage(&mut env_priv.storage);
    }

    #[test]
    fn emacs_value_storage_gc_roots_trace_initial_and_overflow_frames() {
        let mut env_priv = emacs_env_private::new_boxed();
        let count = VALUE_FRAME_SIZE * 2 + 17;
        for i in 0..count {
            let _ = allocate_emacs_value(&mut env_priv.storage, Value::fixnum((i + 1) as i64));
        }
        let priv_ptr: *mut emacs_env_private = &mut *env_priv;
        // SAFETY: `env_priv` stays boxed and live until after `_active` drops.
        let _active = unsafe { ActiveModuleEnv::push(priv_ptr) };
        let heap_id = neovm_value::tagged::gc::current_tagged_heap_identity().unwrap_or(0);
        let mut roots = Vec::new();
        collect_dynamic_module_gc_roots(&mut roots, heap_id);
        let mut seen: Vec<i64> = roots
            .iter()
            .filter_map(|v| v.as_fixnum())
            .filter(|&n| n > 0)
            .collect();
        seen.sort_unstable();
        let expected: Vec<i64> = (1..=(count as i64)).collect();
        assert_eq!(seen, expected);
    }
}
