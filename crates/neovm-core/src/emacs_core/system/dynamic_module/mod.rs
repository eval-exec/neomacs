//! Dynamic module support — GNU Emacs compatible dynamic module API.
//!
//! Architecture mirrors GNU `src/emacs-module.c`.

#![allow(non_camel_case_types)]
#![allow(unsafe_op_in_unsafe_fn)]

use crate::emacs_core::error::LispCondition;
use malachite::integer::Integer;
use std::collections::HashMap;
use std::ffi::{CStr, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Mutex, PoisonError};

use libloading::Library;

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{intern, intern_lisp_string, resolve_sym};
use super::timefns::{LispTimeOutput, make_lisp_time};
use super::value::Value;
use crate::heap_types::LispString;
use crate::tagged::gc::{HeapWriteKind, note_heap_write};
use crate::tagged::header::{ModuleFunctionObj, UserPtrObj, VecLikeType};

// ============================================================================
// C-compatible types matching GNU emacs-module.h
// ============================================================================

#[repr(C)]
pub struct emacs_value_tag {
    pub v: Value,
}

pub type emacs_value = *mut emacs_value_tag;

pub type emacs_function = unsafe extern "C" fn(
    env: *mut emacs_env,
    nargs: isize,
    args: *mut emacs_value,
    data: *mut c_void,
) -> emacs_value;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum emacs_funcall_exit {
    Return = 0,
    Signal = 1,
    Throw = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum emacs_process_input_result {
    Continue = 0,
    Quit = 1,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct emacs_time {
    pub tv_sec: libc::time_t,
    pub tv_nsec: libc::c_long,
}

pub type emacs_limb_t = u64;

#[allow(non_upper_case_globals)]
pub const emacs_variadic_function: isize = -2;

// ============================================================================
// emacs_env — the vtable struct
// ============================================================================

#[repr(C)]
pub struct emacs_env {
    pub size: isize,
    pub private_members: *mut emacs_env_private,

    // Memory management
    pub make_global_ref:
        Option<unsafe extern "C" fn(env: *mut emacs_env, value: emacs_value) -> emacs_value>,
    pub free_global_ref:
        Option<unsafe extern "C" fn(env: *mut emacs_env, global_value: emacs_value)>,

    // Non-local exits
    pub non_local_exit_check:
        Option<unsafe extern "C" fn(env: *mut emacs_env) -> emacs_funcall_exit>,
    pub non_local_exit_clear: Option<unsafe extern "C" fn(env: *mut emacs_env)>,
    pub non_local_exit_get: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            symbol: *mut emacs_value,
            data: *mut emacs_value,
        ) -> emacs_funcall_exit,
    >,
    pub non_local_exit_signal:
        Option<unsafe extern "C" fn(env: *mut emacs_env, symbol: emacs_value, data: emacs_value)>,
    pub non_local_exit_throw:
        Option<unsafe extern "C" fn(env: *mut emacs_env, tag: emacs_value, value: emacs_value)>,

    // Function registration
    pub make_function: Option<
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
    pub is_not_nil: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> bool>,
    pub eq:
        Option<unsafe extern "C" fn(env: *mut emacs_env, a: emacs_value, b: emacs_value) -> bool>,
    pub extract_integer: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> i64>,
    pub make_integer: Option<unsafe extern "C" fn(env: *mut emacs_env, n: i64) -> emacs_value>,
    pub extract_float: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> f64>,
    pub make_float: Option<unsafe extern "C" fn(env: *mut emacs_env, d: f64) -> emacs_value>,

    // Strings
    pub copy_string_contents: Option<
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
    pub make_user_ptr: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            fin: Option<unsafe extern "C" fn(*mut c_void)>,
            ptr: *mut c_void,
        ) -> emacs_value,
    >,
    pub get_user_ptr:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> *mut c_void>,
    pub set_user_ptr:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value, ptr: *mut c_void)>,
    pub get_user_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            uptr: emacs_value,
        ) -> Option<unsafe extern "C" fn(*mut c_void)>,
    >,
    pub set_user_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
            fin: Option<unsafe extern "C" fn(*mut c_void)>,
        ),
    >,

    // Vectors
    pub vec_get: Option<
        unsafe extern "C" fn(env: *mut emacs_env, vector: emacs_value, index: isize) -> emacs_value,
    >,
    pub vec_set: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            vector: emacs_value,
            index: isize,
            value: emacs_value,
        ),
    >,
    pub vec_size: Option<unsafe extern "C" fn(env: *mut emacs_env, vector: emacs_value) -> isize>,

    // Quit/input
    pub should_quit: Option<unsafe extern "C" fn(env: *mut emacs_env) -> bool>,
    pub process_input:
        Option<unsafe extern "C" fn(env: *mut emacs_env) -> emacs_process_input_result>,

    // Time
    pub extract_time:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> emacs_time>,
    pub make_time:
        Option<unsafe extern "C" fn(env: *mut emacs_env, time: emacs_time) -> emacs_value>,

    // Big integer
    pub extract_big_integer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
            sign: *mut std::ffi::c_int,
            count: *mut isize,
            magnitude: *mut emacs_limb_t,
        ) -> bool,
    >,
    pub make_big_integer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            sign: std::ffi::c_int,
            count: isize,
            magnitude: *const emacs_limb_t,
        ) -> emacs_value,
    >,

    // Function finalizer
    pub get_function_finalizer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
        ) -> Option<unsafe extern "C" fn(*mut c_void)>,
    >,
    pub set_function_finalizer: Option<
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
    pub make_interactive:
        Option<unsafe extern "C" fn(env: *mut emacs_env, function: emacs_value, spec: emacs_value)>,

    // Unibyte string, added at the end of GNU emacs_env_31.
    pub make_unibyte_string: Option<
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

pub struct emacs_runtime_private {
    pub env: *mut emacs_env,
}

#[repr(C)]
pub struct emacs_runtime {
    pub size: isize,
    pub private_members: *mut emacs_runtime_private,
    pub get_environment:
        Option<unsafe extern "C" fn(runtime: *mut emacs_runtime) -> *mut emacs_env>,
}

// ============================================================================
// Value arena
// ============================================================================

const VALUE_FRAME_SIZE: usize = 512;

#[repr(C)]
pub struct emacs_value_frame {
    pub objects: [emacs_value_tag; VALUE_FRAME_SIZE],
    pub offset: usize,
    pub next: *mut emacs_value_frame,
}

pub struct emacs_value_storage {
    pub initial: emacs_value_frame,
    pub current: *mut emacs_value_frame,
}

// ============================================================================
// emacs_env_private
// ============================================================================

pub struct emacs_env_private {
    pub pending_non_local_exit: emacs_funcall_exit,
    pub non_local_exit_symbol: Value,
    pub non_local_exit_data: Value,
    pub storage: emacs_value_storage,
}

// ============================================================================
// Global module state
// ============================================================================

pub struct LoadedModule {
    #[allow(dead_code)]
    library: Library,
    #[allow(dead_code)]
    runtime: Box<emacs_runtime>,
    #[allow(dead_code)]
    runtime_priv: Box<emacs_runtime_private>,
    #[allow(dead_code)]
    env: Box<emacs_env>,
    #[allow(dead_code)]
    env_priv: Box<emacs_env_private>,
}

// SAFETY: neomacs runs single-threaded. Module state is never accessed
// from threads other than the main Lisp evaluation thread.
unsafe impl Send for emacs_runtime {}
unsafe impl Sync for emacs_runtime {}
unsafe impl Send for emacs_runtime_private {}
unsafe impl Sync for emacs_runtime_private {}
unsafe impl Send for LoadedModule {}
unsafe impl Sync for LoadedModule {}

static LOADED_MODULES: Mutex<Option<HashMap<String, LoadedModule>>> = Mutex::new(None);

thread_local! {
    static GLOBAL_REFS: std::cell::RefCell<Vec<Option<GlobalRefEntry>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static ACTIVE_ENVS: std::cell::RefCell<Vec<*mut emacs_env_private>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

struct GlobalRefEntry {
    value: Value,
    refcount: isize,
    tag_ptr: *mut emacs_value_tag,
}

// ============================================================================
// Value arena implementation
// ============================================================================

impl emacs_value_frame {
    fn new() -> Self {
        Self {
            objects: unsafe { std::mem::zeroed() },
            offset: 0,
            next: std::ptr::null_mut(),
        }
    }
}

impl emacs_value_storage {
    fn new() -> Self {
        Self {
            initial: emacs_value_frame::new(),
            current: std::ptr::null_mut(),
        }
    }

    fn init(&mut self) {
        self.current = &mut self.initial;
    }
}

fn allocate_emacs_value(storage: &mut emacs_value_storage, lisp_val: Value) -> emacs_value {
    unsafe {
        let frame = &mut *storage.current;
        if frame.offset >= VALUE_FRAME_SIZE {
            let new_frame = Box::into_raw(Box::new(emacs_value_frame::new()));
            (*new_frame).next = storage.current;
            storage.current = new_frame;
            allocate_emacs_value(storage, lisp_val)
        } else {
            frame.objects[frame.offset] = emacs_value_tag { v: lisp_val };
            let ptr = &mut frame.objects[frame.offset] as *mut emacs_value_tag;
            frame.offset += 1;
            ptr
        }
    }
}

fn value_to_lisp(v: emacs_value) -> Value {
    if v.is_null() {
        return Value::NIL;
    }
    unsafe { (*v).v }
}

unsafe fn finalize_storage(storage: &mut emacs_value_storage) {
    let mut frame = storage.initial.next;
    while !frame.is_null() {
        let next = unsafe { (*frame).next };
        unsafe {
            drop(Box::from_raw(frame));
        }
        frame = next;
    }
    storage.initial.next = std::ptr::null_mut();
    storage.initial.offset = 0;
    storage.current = &mut storage.initial;
}

// ============================================================================
// Module → Value helpers
// ============================================================================

fn lisp_to_value(env: *mut emacs_env, val: Value) -> emacs_value {
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

fn lisp_to_value_ignoring_pending_exit(env: *mut emacs_env, val: Value) -> emacs_value {
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

fn check_pending_non_local_exit(env: *mut emacs_env) -> bool {
    if env.is_null() {
        return false;
    }
    unsafe { (*env).private_members.as_ref() }
        .map(|p| p.pending_non_local_exit != emacs_funcall_exit::Return)
        .unwrap_or(false)
}

fn module_function_begin(env: *mut emacs_env) -> bool {
    !env.is_null() && !check_pending_non_local_exit(env)
}

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

pub(crate) fn collect_dynamic_module_gc_roots(roots: &mut Vec<Value>) {
    GLOBAL_REFS.with(|refs| {
        roots.extend(
            refs.borrow()
                .iter()
                .filter_map(|entry| entry.as_ref().map(|entry| entry.value)),
        );
    });
    ACTIVE_ENVS.with(|envs| {
        for &env_priv in envs.borrow().iter() {
            if env_priv.is_null() {
                continue;
            }
            unsafe {
                let priv_ = &*env_priv;
                roots.push(priv_.non_local_exit_symbol);
                roots.push(priv_.non_local_exit_data);
                let mut frame = &priv_.storage.initial as *const emacs_value_frame;
                while !frame.is_null() {
                    let frame_ref = &*frame;
                    for item in frame_ref.objects.iter().take(frame_ref.offset) {
                        roots.push(item.v);
                    }
                    frame = frame_ref.next;
                }
            }
        }
    });
}

struct ActiveModuleEnv {
    env_priv: *mut emacs_env_private,
}

impl ActiveModuleEnv {
    fn push(env_priv: *mut emacs_env_private) -> Self {
        ACTIVE_ENVS.with(|envs| envs.borrow_mut().push(env_priv));
        Self { env_priv }
    }
}

impl Drop for ActiveModuleEnv {
    fn drop(&mut self) {
        ACTIVE_ENVS.with(|envs| {
            let mut envs = envs.borrow_mut();
            let last = envs
                .pop()
                .expect("active module environment stack underflow");
            debug_assert_eq!(last, self.env_priv);
        });
    }
}

unsafe fn module_handle_nonlocal_exit(env: *mut emacs_env, flow: Flow) {
    unsafe {
        let priv_ = &mut *(*env).private_members;
        match flow {
            Flow::Signal(sig) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::from_sym_id(sig.symbol);
                priv_.non_local_exit_data = sig
                    .raw_data
                    .unwrap_or_else(|| Value::list(sig.data.clone()));
            }
            Flow::Throw(thrown) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Throw;
                priv_.non_local_exit_symbol = thrown.tag;
                priv_.non_local_exit_data = thrown.value;
            }
            Flow::ThreadBlocked(_) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::symbol("error");
                priv_.non_local_exit_data =
                    Value::list(vec![Value::string("Thread blocked inside module call")]);
            }
            // The module ABI has no shutdown exit kind. Report it as a signal
            // so the module's own error path runs; the shutdown request is
            // already recorded, so the exit still happens once control returns
            // to the evaluator.
            Flow::Shutdown(request) => {
                priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
                priv_.non_local_exit_symbol = Value::symbol("kill-emacs");
                priv_.non_local_exit_data =
                    Value::list(vec![Value::fixnum(i64::from(request.exit_code))]);
            }
        }
    }
}

fn module_signal_or_throw(priv_: &emacs_env_private) -> Result<(), Flow> {
    match priv_.pending_non_local_exit {
        emacs_funcall_exit::Return => Ok(()),
        emacs_funcall_exit::Signal => {
            let sym_id = priv_
                .non_local_exit_symbol
                .as_symbol_id()
                .unwrap_or_else(|| intern("error"));
            let sym_name = resolve_sym(sym_id);
            let raw_data = priv_.non_local_exit_data;
            Err(Flow::Signal(Box::new(super::error::SignalData::new(
                intern(sym_name),
                super::value::list_to_vec(&raw_data).unwrap_or_else(|| vec![raw_data]),
                Some(raw_data),
                false,
            ))))
        }
        emacs_funcall_exit::Throw => Err(Flow::throw(
            priv_.non_local_exit_symbol,
            priv_.non_local_exit_data,
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

/// Best-effort text from a caught panic payload. Shared with the JIT shim
/// boundary (`jit_shim_contain!` in jit/compile.rs).
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<opaque panic payload>".to_string()
    }
}

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
            return crate::tagged::gc::with_tagged_heap(|h| h.gc_locks_poisoned());
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
fn module_guard<R>(env: *mut emacs_env, sentinel: R, body: impl FnOnce() -> R) -> R {
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

unsafe fn initialize_environment(env: *mut emacs_env, priv_: *mut emacs_env_private) {
    unsafe {
        (*priv_).pending_non_local_exit = emacs_funcall_exit::Return;
        (*priv_).non_local_exit_symbol = Value::NIL;
        (*priv_).non_local_exit_data = Value::NIL;
        (*priv_).storage.init();

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

unsafe extern "C" fn module_make_global_ref(
    env: *mut emacs_env,
    value: emacs_value,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if value.is_null() {
            return std::ptr::null_mut();
        }
        let lisp_val = value_to_lisp(value);
        if check_pending_non_local_exit(env) {
            return std::ptr::null_mut();
        }
        GLOBAL_REFS.with(|refs| {
            let mut refs = refs.borrow_mut();
            for entry_opt in refs.iter_mut() {
                if let Some(entry) = entry_opt
                    && entry.value == lisp_val
                {
                    entry.refcount += 1;
                    return entry.tag_ptr;
                }
            }
            let tag = Box::into_raw(Box::new(emacs_value_tag { v: lisp_val }));
            let entry = GlobalRefEntry {
                value: lisp_val,
                refcount: 1,
                tag_ptr: tag,
            };
            if let Some(pos) = refs.iter().position(|e| e.is_none()) {
                refs[pos] = Some(entry);
            } else {
                refs.push(Some(entry));
            }
            tag
        })
    })
}

unsafe extern "C" fn module_free_global_ref(_env: *mut emacs_env, global_value: emacs_value) {
    module_guard(_env, (), || {
        if global_value.is_null() {
            return;
        }
        GLOBAL_REFS.with(|refs| {
            let mut refs = refs.borrow_mut();
            for entry_opt in refs.iter_mut() {
                if let Some(entry) = entry_opt
                    && entry.tag_ptr == global_value
                {
                    entry.refcount -= 1;
                    if entry.refcount <= 0 {
                        drop(Box::from_raw(global_value));
                        *entry_opt = None;
                    }
                    return;
                }
            }
        });
    })
}

// --- Non-local exits ---

unsafe extern "C" fn module_non_local_exit_check(env: *mut emacs_env) -> emacs_funcall_exit {
    module_guard(env, emacs_funcall_exit::Return, || {
        if env.is_null() {
            return emacs_funcall_exit::Return;
        }
        unsafe { (*env).private_members.as_ref() }
            .map(|p| p.pending_non_local_exit)
            .unwrap_or(emacs_funcall_exit::Return)
    })
}

unsafe extern "C" fn module_non_local_exit_clear(env: *mut emacs_env) {
    module_guard(env, (), || {
        if env.is_null() {
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Return;
            priv_.non_local_exit_symbol = Value::NIL;
            priv_.non_local_exit_data = Value::NIL;
        }
    })
}

unsafe extern "C" fn module_non_local_exit_get(
    env: *mut emacs_env,
    symbol: *mut emacs_value,
    data: *mut emacs_value,
) -> emacs_funcall_exit {
    module_guard(env, emacs_funcall_exit::Return, || {
        if env.is_null() {
            return emacs_funcall_exit::Return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            let status = priv_.pending_non_local_exit;
            if status != emacs_funcall_exit::Return {
                if !symbol.is_null() {
                    *symbol = lisp_to_value_ignoring_pending_exit(env, priv_.non_local_exit_symbol);
                }
                if !data.is_null() {
                    *data = lisp_to_value_ignoring_pending_exit(env, priv_.non_local_exit_data);
                }
            }
            status
        }
    })
}

unsafe extern "C" fn module_non_local_exit_signal(
    env: *mut emacs_env,
    symbol: emacs_value,
    data: emacs_value,
) {
    module_guard(env, (), || {
        if env.is_null() {
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Signal;
            priv_.non_local_exit_symbol = value_to_lisp(symbol);
            priv_.non_local_exit_data = value_to_lisp(data);
        }
    })
}

unsafe extern "C" fn module_non_local_exit_throw(
    env: *mut emacs_env,
    tag: emacs_value,
    value: emacs_value,
) {
    module_guard(env, (), || {
        if env.is_null() {
            return;
        }
        unsafe {
            let priv_ = &mut *(*env).private_members;
            priv_.pending_non_local_exit = emacs_funcall_exit::Throw;
            priv_.non_local_exit_symbol = value_to_lisp(tag);
            priv_.non_local_exit_data = value_to_lisp(value);
        }
    })
}

// --- Intern ---

unsafe extern "C" fn module_intern(
    env: *mut emacs_env,
    name: *const std::ffi::c_char,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if name.is_null() || !module_function_begin(env) {
            return std::ptr::null_mut();
        }
        let cstr = unsafe { CStr::from_ptr(name) };
        let name = LispString::from_emacs_bytes(cstr.to_bytes().to_vec());
        let val = Value::from_sym_id(intern_lisp_string(&name));
        lisp_to_value(env, val)
    })
}

// --- Type conversion ---

unsafe extern "C" fn module_type_of(env: *mut emacs_env, arg: emacs_value) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) || arg.is_null() {
            return std::ptr::null_mut();
        }
        let val = value_to_lisp(arg);
        let type_name: &str = match val.kind() {
            super::value::ValueKind::Nil | super::value::ValueKind::T => "symbol",
            super::value::ValueKind::Fixnum(_) => "integer",
            super::value::ValueKind::Symbol(_) => "symbol",
            super::value::ValueKind::Cons => "cons",
            super::value::ValueKind::String => "string",
            super::value::ValueKind::Float => "float",
            super::value::ValueKind::Subr(_) => "primitive-function",
            super::value::ValueKind::Veclike(vt) => match vt {
                VecLikeType::Vector => "vector",
                VecLikeType::HashTable => "hash-table",
                VecLikeType::Obarray => "obarray",
                VecLikeType::Lambda | VecLikeType::Macro => "interpreted-function",
                VecLikeType::ByteCode => "byte-code-function",
                VecLikeType::Record => "record",
                VecLikeType::Font => "font-object",
                VecLikeType::WindowConfiguration => "window-configuration",
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
                VecLikeType::SurfaceHandle => "neomacs-surface",
                VecLikeType::Subr => "primitive-function",
                VecLikeType::Bignum => "bignum",
                VecLikeType::SymbolWithPos => "symbol-with-pos",
                VecLikeType::Finalizer => "finalizer",
                VecLikeType::Sqlite => "sqlite",
                VecLikeType::UserPtr => "user-ptr",
                VecLikeType::ModuleFunction => "module-function",
                VecLikeType::CharTable => "char-table",
                VecLikeType::SubCharTable => "sub-char-table",
            },
            _ => "unknown",
        };
        let sym = Value::from_sym_id(intern(type_name));
        lisp_to_value(env, sym)
    })
}

unsafe extern "C" fn module_is_not_nil(env: *mut emacs_env, arg: emacs_value) -> bool {
    module_guard(env, false, || {
        if !module_function_begin(env) || arg.is_null() {
            return false;
        }
        !value_to_lisp(arg).is_nil()
    })
}

unsafe extern "C" fn module_eq(env: *mut emacs_env, a: emacs_value, b: emacs_value) -> bool {
    module_guard(env, false, || {
        if !module_function_begin(env) || a.is_null() || b.is_null() {
            return false;
        }
        value_to_lisp(a) == value_to_lisp(b)
    })
}

unsafe extern "C" fn module_extract_integer(env: *mut emacs_env, arg: emacs_value) -> i64 {
    module_guard(env, 0, || {
        if !module_function_begin(env) || arg.is_null() {
            return 0;
        }
        let val = value_to_lisp(arg);
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
    })
}

unsafe extern "C" fn module_make_integer(env: *mut emacs_env, n: i64) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) {
            return std::ptr::null_mut();
        }
        lisp_to_value(env, Value::fixnum(n))
    })
}

unsafe extern "C" fn module_extract_float(env: *mut emacs_env, arg: emacs_value) -> f64 {
    module_guard(env, 0.0, || {
        if !module_function_begin(env) || arg.is_null() {
            return 0.0;
        }
        let val = value_to_lisp(arg);
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
    })
}

unsafe extern "C" fn module_make_float(env: *mut emacs_env, d: f64) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) {
            return std::ptr::null_mut();
        }
        lisp_to_value(env, Value::make_float(d))
    })
}

// --- String ---

unsafe extern "C" fn module_copy_string_contents(
    env: *mut emacs_env,
    value: emacs_value,
    buf: *mut std::ffi::c_char,
    len: *mut isize,
) -> bool {
    module_guard(env, false, || {
        if !module_function_begin(env) || value.is_null() || len.is_null() {
            return false;
        }
        let val = value_to_lisp(value);
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
    })
}

unsafe extern "C" fn module_make_string(
    env: *mut emacs_env,
    str: *const std::ffi::c_char,
    len: isize,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) || len < 0 {
            return std::ptr::null_mut();
        }
        if len > 0 && str.is_null() {
            return std::ptr::null_mut();
        }
        let bytes = unsafe { std::slice::from_raw_parts(str as *const u8, len as usize) };
        let s = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => {
                unsafe {
                    set_pending_signal(
                        env,
                        "wrong-type-argument",
                        Value::list(vec![
                            Value::symbol("utf-8-string-p"),
                            Value::heap_string(crate::heap_types::LispString::from_unibyte(
                                bytes.to_vec(),
                            )),
                        ]),
                    );
                }
                return std::ptr::null_mut();
            }
        };
        lisp_to_value(
            env,
            Value::heap_string(crate::heap_types::LispString::from_utf8(s)),
        )
    })
}

unsafe extern "C" fn module_make_unibyte_string(
    env: *mut emacs_env,
    str: *const std::ffi::c_char,
    len: isize,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) || len < 0 {
            return std::ptr::null_mut();
        }
        if len > 0 && str.is_null() {
            return std::ptr::null_mut();
        }
        let bytes = unsafe { std::slice::from_raw_parts(str as *const u8, len as usize) };
        let val = crate::tagged::gc::with_tagged_heap(|h| {
            h.alloc_string(crate::heap_types::LispString::from_unibyte(bytes.to_vec()))
        });
        lisp_to_value(env, val)
    })
}

// --- User pointer ---

unsafe extern "C" fn module_make_user_ptr(
    env: *mut emacs_env,
    fin: Option<unsafe extern "C" fn(*mut c_void)>,
    ptr: *mut c_void,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) {
            return std::ptr::null_mut();
        }
        let val = crate::tagged::gc::with_tagged_heap(|h| h.alloc_user_ptr(ptr, fin));
        lisp_to_value(env, val)
    })
}

unsafe extern "C" fn module_get_user_ptr(env: *mut emacs_env, arg: emacs_value) -> *mut c_void {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) || arg.is_null() {
            return std::ptr::null_mut();
        }
        let val = value_to_lisp(arg);
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
    })
}

unsafe extern "C" fn module_set_user_ptr(env: *mut emacs_env, arg: emacs_value, ptr: *mut c_void) {
    module_guard(env, (), || {
        if !module_function_begin(env) || arg.is_null() {
            return;
        }
        let val = value_to_lisp(arg);
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
    })
}

unsafe extern "C" fn module_get_user_finalizer(
    env: *mut emacs_env,
    uptr: emacs_value,
) -> Option<unsafe extern "C" fn(*mut c_void)> {
    module_guard(env, None, || {
        if !module_function_begin(env) || uptr.is_null() {
            return None;
        }
        let val = value_to_lisp(uptr);
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
    })
}

unsafe extern "C" fn module_set_user_finalizer(
    env: *mut emacs_env,
    arg: emacs_value,
    fin: Option<unsafe extern "C" fn(*mut c_void)>,
) {
    module_guard(env, (), || {
        if !module_function_begin(env) || arg.is_null() {
            return;
        }
        let val = value_to_lisp(arg);
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
    })
}

// --- Vector ---

unsafe extern "C" fn module_vec_get(
    env: *mut emacs_env,
    vector: emacs_value,
    index: isize,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) || vector.is_null() || index < 0 {
            return std::ptr::null_mut();
        }
        let val = value_to_lisp(vector);
        if let Some(slice) = val.as_vector_data() {
            let idx = index as usize;
            if idx < slice.len() {
                return lisp_to_value(env, slice[idx]);
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
    })
}

unsafe extern "C" fn module_vec_set(
    env: *mut emacs_env,
    vector: emacs_value,
    index: isize,
    value: emacs_value,
) {
    module_guard(env, (), || {
        if !module_function_begin(env) || vector.is_null() || index < 0 {
            return;
        }
        let val = value_to_lisp(vector);
        if val.veclike_type() == Some(VecLikeType::Vector) {
            let idx = index as usize;
            // Route the slot store through the barriered chokepoint. Writing the
            // slot raw (as this did) skips the SATB pre-write log, so a concurrent
            // mark could sweep an object whose last live reference was the
            // overwritten slot — a use-after-free. `set_vector_slot` performs the
            // bounds check + `note_heap_slot_write` + atomic store, returning false
            // here only when `idx` is out of bounds.
            if val.set_vector_slot(idx, value_to_lisp(value)) {
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
    })
}

unsafe extern "C" fn module_vec_size(env: *mut emacs_env, vector: emacs_value) -> isize {
    module_guard(env, 0, || {
        if !module_function_begin(env) || vector.is_null() {
            return 0;
        }
        let val = value_to_lisp(vector);
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
    })
}

// --- Quit / Input ---

unsafe extern "C" fn module_should_quit(env: *mut emacs_env) -> bool {
    module_guard(env, false, || {
        if env.is_null() || check_pending_non_local_exit(env) {
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
    })
}

unsafe extern "C" fn module_process_input(env: *mut emacs_env) -> emacs_process_input_result {
    module_guard(env, emacs_process_input_result::Quit, || {
        if !module_function_begin(env) {
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
    })
}

// --- Time ---

unsafe extern "C" fn module_extract_time(env: *mut emacs_env, arg: emacs_value) -> emacs_time {
    module_guard(
        env,
        emacs_time {
            tv_sec: 0,
            tv_nsec: 0,
        },
        || {
            if !module_function_begin(env) || arg.is_null() {
                return emacs_time {
                    tv_sec: 0,
                    tv_nsec: 0,
                };
            }
            let val = value_to_lisp(arg);
            if let Some(items) = val.as_vector_data()
                && items.len() >= 2
            {
                let sec_high = items[0].as_fixnum().unwrap_or(0);
                let sec_low = items[1].as_fixnum().unwrap_or(0);
                let micros = if items.len() >= 3 {
                    items[2].as_fixnum().unwrap_or(0)
                } else {
                    0
                };
                let picos = if items.len() >= 4 {
                    items[3].as_fixnum().unwrap_or(0)
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
        },
    )
}

unsafe extern "C" fn module_make_time(env: *mut emacs_env, time: emacs_time) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) {
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
        lisp_to_value(env, lisp_time)
    })
}

// --- Big integer ---

unsafe extern "C" fn module_extract_big_integer(
    env: *mut emacs_env,
    arg: emacs_value,
    sign: *mut std::ffi::c_int,
    count: *mut isize,
    magnitude: *mut emacs_limb_t,
) -> bool {
    module_guard(env, false, || {
        if !module_function_begin(env) || arg.is_null() {
            return false;
        }
        let val = value_to_lisp(arg);
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
    })
}

unsafe extern "C" fn module_make_big_integer(
    env: *mut emacs_env,
    sign: std::ffi::c_int,
    count: isize,
    magnitude: *const emacs_limb_t,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) {
            return std::ptr::null_mut();
        }
        if sign == 0 {
            return lisp_to_value(env, Value::fixnum(0));
        }
        if count == 0 {
            return lisp_to_value(env, Value::fixnum(0));
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
                return lisp_to_value(env, Value::make_int(v));
            }
        }
        let b = Integer::from_twos_complement_limbs_asc(limbs);
        let val = Value::make_integer(if sign >= 0 { b } else { -b });
        lisp_to_value(env, val)
    })
}

// --- Function finalizer ---

unsafe extern "C" fn module_get_function_finalizer(
    env: *mut emacs_env,
    arg: emacs_value,
) -> Option<unsafe extern "C" fn(*mut c_void)> {
    module_guard(env, None, || {
        if !module_function_begin(env) || arg.is_null() {
            return None;
        }
        let val = value_to_lisp(arg);
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
    })
}

unsafe extern "C" fn module_set_function_finalizer(
    env: *mut emacs_env,
    arg: emacs_value,
    fin: Option<unsafe extern "C" fn(*mut c_void)>,
) {
    module_guard(env, (), || {
        if !module_function_begin(env) || arg.is_null() {
            return;
        }
        let val = value_to_lisp(arg);
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
    })
}

// --- Pipe channel ---

unsafe extern "C" fn module_open_channel(
    env: *mut emacs_env,
    pipe_process: emacs_value,
) -> std::ffi::c_int {
    module_guard(env, -1, || {
        if !module_function_begin(env) || pipe_process.is_null() {
            return -1;
        }
        let process = value_to_lisp(pipe_process);
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
    })
}

// --- Interactive ---

unsafe extern "C" fn module_make_interactive(
    env: *mut emacs_env,
    function: emacs_value,
    spec: emacs_value,
) {
    module_guard(env, (), || {
        if !module_function_begin(env) || function.is_null() {
            return;
        }
        let func_val = value_to_lisp(function);
        let spec_val = value_to_lisp(spec);
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
    })
}

// ============================================================================
// make_function
// ============================================================================

unsafe extern "C" fn module_make_function(
    env: *mut emacs_env,
    min_arity: isize,
    max_arity: isize,
    subr: emacs_function,
    docstring: *const std::ffi::c_char,
    data: *mut c_void,
) -> emacs_value {
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) {
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
                Ok(s) => Value::heap_string(crate::heap_types::LispString::from_utf8(s)),
                Err(_) => {
                    unsafe {
                        set_pending_signal(
                            env,
                            "wrong-type-argument",
                            Value::list(vec![
                                Value::symbol("utf-8-string-p"),
                                Value::heap_string(crate::heap_types::LispString::from_unibyte(
                                    cstr.to_bytes().to_vec(),
                                )),
                            ]),
                        );
                    }
                    return std::ptr::null_mut();
                }
            }
        };
        let val = crate::tagged::gc::with_tagged_heap(|h| {
            h.alloc_module_function(
                min_arity,
                max_arity,
                subr as *const c_void,
                data,
                doc_val,
                Value::NIL,
            )
        });
        lisp_to_value(env, val)
    })
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
    module_guard(env, std::ptr::null_mut(), || {
        if !module_function_begin(env) || func.is_null() {
            return std::ptr::null_mut();
        }
        let func_val = value_to_lisp(func);
        let mut lisp_args = Vec::with_capacity(nargs as usize);
        for i in 0..nargs as usize {
            let arg = unsafe { *args.add(i) };
            lisp_args.push(value_to_lisp(arg));
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
            Ok(ret_val) => lisp_to_value(env, ret_val),
            Err(flow) => {
                unsafe {
                    module_handle_nonlocal_exit(env, flow);
                }
                std::ptr::null_mut()
            }
        }
    })
}

// ============================================================================
// Thread-local context pointer for FFI callbacks
// ============================================================================

thread_local! {
    static MODULE_CTX: std::cell::Cell<*mut Context> =
        const { std::cell::Cell::new(std::ptr::null_mut()) };
}

/// RAII installer for `MODULE_CTX`: saves the previous pointer on
/// construction and restores it on `Drop` (precedent: [`ActiveModuleEnv`]).
/// Restoring — rather than clearing to NULL — is what makes nested
/// module→elisp→module calls work: when the inner `apply_module_function`
/// returns, the outer call's context must come back, or the outer module
/// function's next `env->funcall` finds no evaluator context. `Drop` also
/// keeps the teardown coherent once panics become catchable.
struct ModuleContextGuard {
    prev: *mut Context,
}

impl ModuleContextGuard {
    fn install(ctx: *mut Context) -> Self {
        Self {
            prev: MODULE_CTX.with(|c| c.replace(ctx)),
        }
    }
}

impl Drop for ModuleContextGuard {
    fn drop(&mut self) {
        MODULE_CTX.with(|c| c.set(self.prev));
    }
}

// ============================================================================
// emacs_runtime get_environment
// ============================================================================

unsafe extern "C" fn module_get_environment(rt: *mut emacs_runtime) -> *mut emacs_env {
    // No env exists yet to record a pending exit on; a caught panic here just
    // returns NULL (which module init code must treat as failure anyway).
    module_guard(std::ptr::null_mut(), std::ptr::null_mut(), || {
        if rt.is_null() {
            return std::ptr::null_mut();
        }
        unsafe {
            let priv_ = &*(*rt).private_members;
            priv_.env
        }
    })
}

// ============================================================================
// module-load entry point
// ============================================================================

pub fn load_module(ctx: &mut Context, path: std::path::PathBuf) -> EvalResult {
    // Load the shared library from the byte-faithful path (eight-bit-safe on
    // Unix); `path_str` is a display form used only for error text and the
    // already-loaded registry key (module paths are ASCII in practice).
    let path_str = path.display().to_string();
    let lib = unsafe { Library::new(&path) }.map_err(|e| {
        signal(
            "module-open-failed",
            vec![Value::string(&path_str), Value::string(e.to_string())],
        )
    })?;

    {
        let has_gpl: Result<libloading::Symbol<*const ()>, _> =
            unsafe { lib.get(b"plugin_is_GPL_compatible") };
        if has_gpl.is_err() {
            return Err(signal(
                "module-not-gpl-compatible",
                vec![Value::string(&path_str)],
            ));
        }
    }

    type EmacsInitFn = unsafe extern "C" fn(runtime: *mut emacs_runtime) -> std::ffi::c_int;
    let init_fn: libloading::Symbol<EmacsInitFn> = unsafe { lib.get(b"emacs_module_init") }
        .map_err(|_| {
            signal(
                "missing-module-init-function",
                vec![Value::string(&path_str)],
            )
        })?;
    let init_fn_ptr: EmacsInitFn = *init_fn;

    let mut env_priv = Box::new(emacs_env_private {
        pending_non_local_exit: emacs_funcall_exit::Return,
        non_local_exit_symbol: Value::NIL,
        non_local_exit_data: Value::NIL,
        storage: emacs_value_storage::new(),
    });
    env_priv.storage.init();
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
    unsafe {
        initialize_environment(env_ptr, env_priv_ptr);
    }

    let rt_priv = Box::new(emacs_runtime_private { env: env_ptr });
    let mut rt = Box::new(emacs_runtime {
        size: std::mem::size_of::<emacs_runtime>() as isize,
        private_members: Box::into_raw(rt_priv),
        get_environment: Some(module_get_environment),
    });

    let module_ctx = ModuleContextGuard::install(ctx as *mut Context);
    let active_env = ActiveModuleEnv::push(env_priv_ptr);
    let init_code = unsafe { init_fn_ptr(&mut *rt as *mut emacs_runtime) };
    drop(module_ctx);

    ctx.maybe_quit()?;
    let env_priv_ref = unsafe { &*env_priv_ptr };
    module_signal_or_throw(env_priv_ref)?;
    drop(active_env);

    if init_code != 0 {
        return Err(signal(
            "module-init-failed",
            vec![Value::string(&path_str), Value::fixnum(init_code as i64)],
        ));
    }

    let rt_priv_reconstructed = unsafe { Box::from_raw(rt.private_members) };

    let loaded = LoadedModule {
        library: lib,
        runtime: rt,
        runtime_priv: rt_priv_reconstructed,
        env: env_box,
        env_priv,
    };

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

    let subr_fn: emacs_function = unsafe { std::mem::transmute(mf.subr) };
    let data = mf.data;

    let mut env_priv = Box::new(emacs_env_private {
        pending_non_local_exit: emacs_funcall_exit::Return,
        non_local_exit_symbol: Value::NIL,
        non_local_exit_data: Value::NIL,
        storage: emacs_value_storage::new(),
    });
    env_priv.storage.init();

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
        emacs_args.push(lisp_to_value(env_ptr, *arg));
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
    let module_ctx = ModuleContextGuard::install(ctx as *mut Context);
    let active_env = ActiveModuleEnv::push(priv_ptr);
    let args_ptr = emacs_args.as_mut_ptr();
    let call = catch_unwind(AssertUnwindSafe(|| unsafe {
        subr_fn_unwind(env_ptr, nargs, args_ptr, data)
    }));
    drop(module_ctx);

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
    let ret = value_to_lisp(result);
    // A contained panic outranks any exit the module recorded before dying:
    // the env's pending state was part of the computation the panic tore down.
    let exit = match panic_flow {
        Some(flow) => Err(flow),
        None => module_signal_or_throw(priv_ref),
    };
    drop(active_env);

    unsafe {
        finalize_storage(&mut env_priv.storage);
    }
    drop(env);
    drop(env_priv);

    exit?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emacs_core::value::list_to_vec;

    unsafe extern "C" fn dummy_module_function(
        _env: *mut emacs_env,
        _nargs: isize,
        _args: *mut emacs_value,
        _data: *mut c_void,
    ) -> emacs_value {
        std::ptr::null_mut()
    }

    struct TestEnv {
        env: Box<emacs_env>,
        priv_: Box<emacs_env_private>,
    }

    impl TestEnv {
        fn new() -> Self {
            let mut priv_ = Box::new(emacs_env_private {
                pending_non_local_exit: emacs_funcall_exit::Return,
                non_local_exit_symbol: Value::NIL,
                non_local_exit_data: Value::NIL,
                storage: emacs_value_storage::new(),
            });
            let mut env = Box::new(unsafe { std::mem::zeroed::<emacs_env>() });
            let env_ptr = &mut *env as *mut emacs_env;
            let priv_ptr = &mut *priv_ as *mut emacs_env_private;
            unsafe {
                initialize_environment(env_ptr, priv_ptr);
            }
            Self { env, priv_ }
        }

        fn env_ptr(&mut self) -> *mut emacs_env {
            &mut *self.env
        }

        fn priv_ptr(&mut self) -> *mut emacs_env_private {
            &mut *self.priv_
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            unsafe {
                finalize_storage(&mut self.priv_.storage);
            }
        }
    }

    /// `ModuleContextGuard` must restore the PREVIOUS context on drop —
    /// nested module→elisp→module calls depend on the outer pointer coming
    /// back — and must do the same when its scope unwinds.
    #[test]
    fn module_context_guard_restores_previous_on_drop_and_panic() {
        // Fake, never-dereferenced pointers: the guard only moves them in
        // and out of the MODULE_CTX cell.
        let outer = 0x1000 as *mut Context;
        let inner = 0x2000 as *mut Context;
        let current = || MODULE_CTX.with(|c| c.get());

        assert!(current().is_null());
        {
            let _outer_guard = ModuleContextGuard::install(outer);
            assert_eq!(current(), outer);
            {
                let _inner_guard = ModuleContextGuard::install(inner);
                assert_eq!(current(), inner);
            }
            assert_eq!(
                current(),
                outer,
                "inner drop must restore the outer context"
            );
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = ModuleContextGuard::install(inner);
                panic!("boom under module context");
            }));
            assert!(panicked.is_err());
            assert_eq!(
                current(),
                outer,
                "unwinding must restore the previous context"
            );
        }
        assert!(current().is_null());
    }

    #[test]
    fn module_make_interactive_wraps_specs_like_gnu() {
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();
        let func = unsafe {
            module_make_function(
                env,
                0,
                0,
                dummy_module_function,
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };

        unsafe {
            module_make_interactive(env, func, lisp_to_value(env, Value::NIL));
        }
        let form = value_to_lisp(func)
            .as_module_function()
            .unwrap()
            .interactive_form;
        assert_eq!(
            list_to_vec(&form).unwrap(),
            vec![Value::symbol("interactive")]
        );

        let spec = lisp_to_value(env, Value::string("p"));
        unsafe {
            module_make_interactive(env, func, spec);
        }
        let form = value_to_lisp(func)
            .as_module_function()
            .unwrap()
            .interactive_form;
        assert_eq!(
            list_to_vec(&form).unwrap(),
            vec![Value::symbol("interactive"), Value::string("p")]
        );
    }

    #[test]
    fn active_module_environment_values_are_gc_roots() {
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();
        let priv_ptr = fixture.priv_ptr();
        let rooted = Value::string("module-local-root");
        let _value = lisp_to_value(env, rooted);
        unsafe {
            (*priv_ptr).pending_non_local_exit = emacs_funcall_exit::Signal;
            (*priv_ptr).non_local_exit_symbol = Value::symbol("error");
            (*priv_ptr).non_local_exit_data = Value::list(vec![rooted]);
        }

        let active = ActiveModuleEnv::push(priv_ptr);
        let mut roots = Vec::new();
        collect_dynamic_module_gc_roots(&mut roots);
        drop(active);

        assert!(roots.contains(&rooted));
        assert!(roots.contains(&Value::symbol("error")));
    }

    #[test]
    fn module_big_integer_zero_and_null_outputs_match_gnu() {
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();

        let zero = unsafe { module_make_big_integer(env, 0, 0, std::ptr::null()) };
        assert_eq!(value_to_lisp(zero), Value::fixnum(0));

        let zero_value = lisp_to_value(env, Value::fixnum(0));
        let ok = unsafe {
            module_extract_big_integer(
                env,
                zero_value,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(ok);
        assert_eq!(
            unsafe { (*fixture.priv_ptr()).pending_non_local_exit },
            emacs_funcall_exit::Return
        );
    }

    #[test]
    fn module_extract_big_integer_reports_too_small_buffer() {
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();
        let large = Value::make_integer(Integer::from(1u64) << 80u32);
        let value = lisp_to_value(env, large);
        let mut sign = 0;
        let mut count = 1;
        let mut magnitude = [0_u64; 1];

        let ok = unsafe {
            module_extract_big_integer(env, value, &mut sign, &mut count, magnitude.as_mut_ptr())
        };

        assert!(!ok);
        assert_eq!(sign, 1);
        assert_eq!(count, 2);
        let priv_ = unsafe { &*fixture.priv_ptr() };
        assert_eq!(priv_.pending_non_local_exit, emacs_funcall_exit::Signal);
        assert_eq!(
            priv_.non_local_exit_symbol,
            Value::symbol("memory-buffer-too-small")
        );
    }

    // ------------------------------------------------------------------
    // PS-T4: panic containment at the module ABI boundary
    // ------------------------------------------------------------------

    use crate::emacs_core::eval::{ConditionFrame, ResumeTarget, SpecBinding};

    fn string_of(value: Value) -> String {
        String::from_utf8_lossy(value.as_lisp_string().expect("a string").as_bytes()).into_owned()
    }

    /// A host-defined module function whose panic must be caught by
    /// `apply_module_function` — the same-std case containment exists for
    /// (foreign Rust modules abort in their own runtime before reaching us).
    unsafe extern "C-unwind" fn panicking_module_function(
        _env: *mut emacs_env,
        _nargs: isize,
        _args: *mut emacs_value,
        _data: *mut c_void,
    ) -> emacs_value {
        panic!("intentional panic from host module function");
    }

    /// A host-defined module function that runs elisp via `env->funcall`; the
    /// elisp panics (host subr `neovm--internal-panic`), so the panic must be
    /// contained by `module_funcall`'s `contain_lisp_panics`, surface as this
    /// call's pending exit, and propagate out as an ordinary Lisp error.
    unsafe extern "C" fn module_function_calling_panicking_elisp(
        env: *mut emacs_env,
        _nargs: isize,
        _args: *mut emacs_value,
        _data: *mut c_void,
    ) -> emacs_value {
        unsafe {
            let e = &*env;
            let sym = (e.intern.unwrap())(env, c"neovm--internal-panic".as_ptr());
            (e.funcall.unwrap())(env, sym, 0, std::ptr::null_mut())
        }
    }

    fn install_module_function(
        ev: &mut Context,
        name: &str,
        func: unsafe extern "C" fn(
            *mut emacs_env,
            isize,
            *mut emacs_value,
            *mut c_void,
        ) -> emacs_value,
    ) {
        let value = crate::tagged::gc::with_tagged_heap(|h| {
            h.alloc_module_function(
                0,
                0,
                func as *const c_void,
                std::ptr::null_mut(),
                Value::NIL,
                Value::NIL,
            )
        });
        ev.obarray.set_symbol_function(name, value);
    }

    /// `module_guard` must convert a panic into a pending `error` exit whose
    /// message carries the marker + panic text, and return the sentinel.
    #[test]
    fn module_guard_converts_panic_to_pending_error() {
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();

        let out = module_guard(env, 17_i64, || panic!("guard-probe-text"));
        assert_eq!(out, 17);

        let priv_ = unsafe { &*fixture.priv_ptr() };
        assert_eq!(priv_.pending_non_local_exit, emacs_funcall_exit::Signal);
        assert_eq!(priv_.non_local_exit_symbol, Value::symbol("error"));
        let data = list_to_vec(&priv_.non_local_exit_data).unwrap();
        let message = string_of(data[0]);
        assert!(
            message.contains("neomacs internal error") && message.contains("guard-probe-text"),
            "unexpected message: {message}"
        );
    }

    /// First exit wins (GNU convention): an exit recorded before the panic is
    /// preserved by the guard's `set_pending_signal`.
    #[test]
    fn module_guard_preserves_earlier_pending_exit() {
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();
        unsafe {
            set_pending_signal(env, "wrong-type-argument", Value::NIL);
        }

        let _ = module_guard(env, (), || panic!("later panic"));

        let priv_ = unsafe { &*fixture.priv_ptr() };
        assert_eq!(
            priv_.non_local_exit_symbol,
            Value::symbol("wrong-type-argument"),
            "the pre-panic exit must win"
        );
    }

    /// With no MODULE_CTX installed, the guard's probe must still see GC
    /// lock poison through the thread heap (the JIT ctx-less arm's probe)
    /// and refuse to contain: re-raise, no pending exit recorded. Poison is
    /// permanent for this process — fine under nextest's process-per-test.
    #[test]
    fn module_guard_re_raises_on_poisoned_gc_locks_without_ctx() {
        crate::tagged::gc::with_tagged_heap(|h| h.poison_gc_locks_for_test());
        let mut fixture = TestEnv::new();
        let env = fixture.env_ptr();

        let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
            module_guard(env, 17_i64, || panic!("guard-poison-flee"))
        }));
        let payload = caught.expect_err("poisoned GC locks must re-raise, not contain");
        assert_eq!(panic_message(&*payload), "guard-poison-flee");

        let priv_ = unsafe { &*fixture.priv_ptr() };
        assert_eq!(
            priv_.pending_non_local_exit,
            emacs_funcall_exit::Return,
            "no pending exit may be recorded on the re-raise path"
        );
    }

    /// `contain_lisp_panics` must restore every boundary-snapshot dimension
    /// the panicked extent dirtied and surface the panic as an `error` Flow.
    #[test]
    fn contain_lisp_panics_restores_boundary_state() {
        let mut ev = Context::new();
        let spec0 = ev.specpdl.len();
        let cond0 = ev.condition_stack.len();
        let bc0 = ev.bc_buf.len();
        let depth0 = ev.depth;
        let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();

        let result: Result<Value, Flow> = contain_lisp_panics(&mut ev, |ctx| {
            ctx.specpdl.push(SpecBinding::Nop);
            ctx.push_condition_frame(ConditionFrame::Catch {
                tag: Value::symbol("neovm-test-tag"),
                resume: ResumeTarget::InterpreterCatch,
            });
            ctx.bc_buf.push(Value::NIL);
            // Skipped scratch-root pops of the panicked extent: the
            // boundary restore must truncate them (they would pin their
            // objects forever otherwise).
            crate::emacs_core::eval::push_scratch_gc_root(Value::NIL);
            crate::emacs_core::eval::push_scratch_gc_root(Value::T);
            ctx.depth += 3;
            panic!("boundary-dirt-probe");
        });

        let Err(Flow::Signal(sig)) = result else {
            panic!("expected a Signal flow");
        };
        assert_eq!(sig.symbol, intern("error"));
        let message = string_of(sig.data[0]);
        assert!(
            message.contains("neomacs internal error") && message.contains("boundary-dirt-probe"),
            "unexpected message: {message}"
        );
        assert_eq!(ev.specpdl.len(), spec0);
        assert_eq!(ev.condition_stack.len(), cond0);
        assert_eq!(ev.bc_buf.len(), bc0);
        assert_eq!(ev.depth, depth0);
        assert_eq!(
            crate::emacs_core::eval::save_scratch_gc_roots(),
            roots0,
            "scratch-root residue truncated by the boundary restore"
        );
    }

    /// A panic that escaped the GC collection driver must NOT be contained:
    /// the original payload is re-raised (and would abort at the extern "C"
    /// shim in production).
    #[test]
    fn contain_lisp_panics_re_raises_when_gc_driver_was_active() {
        let mut ev = Context::new();
        ev.gc_driver_active = true;

        let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _: Result<Value, Flow> = contain_lisp_panics(&mut ev, |_ctx| panic!("must-flee"));
        }));

        let payload = caught.expect_err("the panic must be re-raised, not contained");
        assert_eq!(panic_message(&*payload), "must-flee");
        ev.gc_driver_active = false;
        assert_eq!(ev.specpdl.len(), 0);
    }

    /// End-to-end over the real dispatch: a panicking module function becomes
    /// a `condition-case`-able `error` carrying the panic text, and afterwards
    /// the evaluator still evaluates and a full GC still runs (Task-1 guard
    /// regression: `gc_inhibit_depth` must be balanced).
    #[test]
    fn module_function_panic_is_condition_case_error_and_evaluator_survives() {
        let mut ev = Context::new();
        install_module_function(&mut ev, "neovm-test--panicking-module-fn", {
            // Coerce the C-unwind fn to the vtable's "C" shape for storage;
            // apply_module_function calls through the C-unwind view.
            unsafe {
                std::mem::transmute::<
                    unsafe extern "C-unwind" fn(
                        *mut emacs_env,
                        isize,
                        *mut emacs_value,
                        *mut c_void,
                    ) -> emacs_value,
                    unsafe extern "C" fn(
                        *mut emacs_env,
                        isize,
                        *mut emacs_value,
                        *mut c_void,
                    ) -> emacs_value,
                >(panicking_module_function)
            }
        });

        let spec0 = ev.specpdl.len();
        let caught = ev
            .eval_str(
                "(condition-case err (neovm-test--panicking-module-fn) (error (car (cdr err))))",
            )
            .expect("condition-case must catch the contained panic");
        let message = string_of(caught);
        assert!(
            message.contains("neomacs internal error")
                && message.contains("intentional panic from host module function"),
            "unexpected message: {message}"
        );

        assert_eq!(ev.specpdl.len(), spec0, "specpdl must be balanced");
        assert_eq!(ev.gc_inhibit_depth, 0, "GC inhibition must be balanced");
        let sum = ev.eval_str("(+ 1 2)").expect("evaluator must still work");
        assert_eq!(sum, Value::fixnum(3));
        ev.eval_str("(garbage-collect)")
            .expect("a full GC must still run after a contained panic");
    }

    /// Panic inside module-INVOKED elisp (the `module_funcall` trampoline):
    /// module code runs `env->funcall` on a host subr that panics; the panic
    /// is contained at `module_funcall`, becomes this call's pending exit, and
    /// propagates to `condition-case` like any Lisp error. Also repeats the
    /// call to prove no poisoned-lock cascade is left behind.
    #[test]
    fn panic_in_module_invoked_elisp_is_contained_at_module_funcall() {
        let mut ev = Context::new();
        install_module_function(
            &mut ev,
            "neovm-test--module-calls-panicking-elisp",
            module_function_calling_panicking_elisp,
        );

        for _ in 0..2 {
            let caught = ev
                .eval_str(
                    "(condition-case err (neovm-test--module-calls-panicking-elisp) \
                       (error (car (cdr err))))",
                )
                .expect("condition-case must catch the contained panic");
            let message = string_of(caught);
            assert!(
                message.contains("neomacs internal error")
                    && message.contains("neovm--internal-panic"),
                "unexpected message: {message}"
            );
            let sum = ev.eval_str("(+ 20 22)").expect("evaluator must still work");
            assert_eq!(sum, Value::fixnum(42));
        }
        assert_eq!(ev.gc_inhibit_depth, 0);
        ev.eval_str("(garbage-collect)")
            .expect("a full GC must still run after a contained panic");
    }
}
