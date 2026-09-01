//! Rust test module for neomacs dynamic module API.
//! Mirrors the key functions from test/src/emacs-module-resources/mod-test.c.

#![allow(non_camel_case_types)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;

// --- FFI types matching dynamic_module.rs exactly ---

#[repr(C)]
pub struct emacs_value_tag {
    pub v: usize,
}
type emacs_value = *mut emacs_value_tag;

#[repr(C)]
#[derive(Clone, Copy)]
pub enum emacs_funcall_exit {
    Return = 0,
    Signal = 1,
    Throw = 2,
}

type emacs_function = unsafe extern "C" fn(
    env: *mut emacs_env,
    nargs: isize,
    args: *mut emacs_value,
    data: *mut c_void,
) -> emacs_value;

#[repr(C)]
pub struct emacs_env {
    pub size: isize,
    pub private_members: *mut c_void,

    pub make_global_ref:
        Option<unsafe extern "C" fn(env: *mut emacs_env, value: emacs_value) -> emacs_value>,
    pub free_global_ref:
        Option<unsafe extern "C" fn(env: *mut emacs_env, global_value: emacs_value)>,

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

    pub type_of: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> emacs_value>,
    pub is_not_nil: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> bool>,
    pub eq:
        Option<unsafe extern "C" fn(env: *mut emacs_env, a: emacs_value, b: emacs_value) -> bool>,
    pub extract_integer: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> i64>,
    pub make_integer: Option<unsafe extern "C" fn(env: *mut emacs_env, n: i64) -> emacs_value>,
    pub extract_float: Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> f64>,
    pub make_float: Option<unsafe extern "C" fn(env: *mut emacs_env, d: f64) -> emacs_value>,

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

    pub should_quit: Option<unsafe extern "C" fn(env: *mut emacs_env) -> bool>,
    pub process_input: Option<unsafe extern "C" fn(env: *mut emacs_env) -> i32>,

    pub extract_time:
        Option<unsafe extern "C" fn(env: *mut emacs_env, arg: emacs_value) -> emacs_time>,
    pub make_time:
        Option<unsafe extern "C" fn(env: *mut emacs_env, time: emacs_time) -> emacs_value>,

    pub extract_big_integer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            arg: emacs_value,
            sign: *mut std::ffi::c_int,
            count: *mut isize,
            magnitude: *mut u64,
        ) -> bool,
    >,
    pub make_big_integer: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            sign: std::ffi::c_int,
            count: isize,
            magnitude: *const u64,
        ) -> emacs_value,
    >,

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

    pub open_channel: Option<
        unsafe extern "C" fn(env: *mut emacs_env, pipe_process: emacs_value) -> std::ffi::c_int,
    >,

    pub make_interactive:
        Option<unsafe extern "C" fn(env: *mut emacs_env, function: emacs_value, spec: emacs_value)>,

    pub make_unibyte_string: Option<
        unsafe extern "C" fn(
            env: *mut emacs_env,
            str: *const std::ffi::c_char,
            len: isize,
        ) -> emacs_value,
    >,
}

#[repr(C)]
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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct emacs_time {
    pub seconds: i64,
    pub nanoseconds: i64,
}

// --- GPL compatibility ---

#[unsafe(no_mangle)]
pub static plugin_is_GPL_compatible: std::ffi::c_int = 0;

// --- Helper: bind a function into the Emacs obarray ---

unsafe fn bind_function(env: *mut emacs_env, name: &str, func: emacs_value) {
    let c_name = std::ffi::CString::new(name).unwrap();
    let sym = env.as_ref().unwrap().intern.unwrap()(env, c_name.as_ptr());
    let fset = env.as_ref().unwrap().intern.unwrap()(env, b"fset\0".as_ptr() as *const _);
    let mut fset_args = [sym, func];
    env.as_ref().unwrap().funcall.unwrap()(env, fset, 2, fset_args.as_mut_ptr());
}

// --- Test functions ---

unsafe extern "C" fn mod_test_return_t(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    env.as_ref().unwrap().intern.unwrap()(env, b"t\0".as_ptr() as *const _)
}

unsafe extern "C" fn mod_test_sum(
    env: *mut emacs_env,
    nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    assert_eq!(nargs, 2);
    let e = env.as_ref().unwrap();
    let a = e.extract_integer.unwrap()(env, *args);
    let b = e.extract_integer.unwrap()(env, *args.add(1));
    e.make_integer.unwrap()(env, a + b)
}

unsafe extern "C" fn mod_test_make_string(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let val = *args;
    let mut buf_len: isize = 0;
    e.copy_string_contents.unwrap()(env, val, std::ptr::null_mut(), &mut buf_len);
    let mut buf = vec![0i8; buf_len as usize];
    let mut actual_len = buf_len;
    e.copy_string_contents.unwrap()(env, val, buf.as_mut_ptr(), &mut actual_len);
    e.make_string.unwrap()(env, buf.as_ptr(), actual_len - 1)
}

unsafe extern "C" fn mod_test_string_a_to_b(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let val = *args;
    let mut buf_len: isize = 0;
    e.copy_string_contents.unwrap()(env, val, std::ptr::null_mut(), &mut buf_len);
    let mut buf = vec![0i8; buf_len as usize];
    let mut actual_len = buf_len;
    e.copy_string_contents.unwrap()(env, val, buf.as_mut_ptr(), &mut actual_len);
    for byte in buf.iter_mut().take((actual_len - 1) as usize) {
        if *byte == 'a' as i8 {
            *byte = 'b' as i8;
        }
    }
    e.make_string.unwrap()(env, buf.as_ptr(), actual_len - 1)
}

static mut FINALIZER_CALLED: bool = false;

unsafe extern "C" fn finalizer_callback(_ptr: *mut c_void) {
    unsafe {
        FINALIZER_CALLED = true;
    }
}

unsafe extern "C" fn mod_test_userptr_make(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let ptr = Box::into_raw(Box::new(42i32)) as *mut c_void;
    e.make_user_ptr.unwrap()(env, Some(finalizer_callback), ptr)
}

unsafe extern "C" fn mod_test_userptr_get(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let ptr = e.get_user_ptr.unwrap()(env, *args);
    if ptr.is_null() {
        return e.intern.unwrap()(env, b"nil\0".as_ptr() as *const _);
    }
    let val: i32 = *(ptr as *const i32);
    // Free the boxed integer.
    drop(Box::from_raw(ptr as *mut i32));
    e.make_integer.unwrap()(env, val as i64)
}

unsafe extern "C" fn mod_test_vector_fill(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let vec = *args;
    let val = *args.add(1);
    let size = e.vec_size.unwrap()(env, vec);
    for i in 0..size {
        e.vec_set.unwrap()(env, vec, i, val);
    }
    vec
}

unsafe extern "C" fn mod_test_vector_eq(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let a = *args;
    let b = *args.add(1);
    let size_a = e.vec_size.unwrap()(env, a);
    let size_b = e.vec_size.unwrap()(env, b);
    if size_a != size_b {
        return e.intern.unwrap()(env, b"nil\0".as_ptr() as *const _);
    }
    for i in 0..size_a {
        let va = e.vec_get.unwrap()(env, a, i);
        let vb = e.vec_get.unwrap()(env, b, i);
        if !e.eq.unwrap()(env, va, vb) {
            return e.intern.unwrap()(env, b"nil\0".as_ptr() as *const _);
        }
    }
    e.intern.unwrap()(env, b"t\0".as_ptr() as *const _)
}

unsafe extern "C" fn mod_test_nested_inner(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    env.as_ref().unwrap().make_integer.unwrap()(env, 21)
}

/// Regression driver for nested module→elisp→module calls: the FIRST
/// env->funcall re-enters the module via an elisp bridge
/// (`mod-test-nested-bridge` calls `mod-test-nested-inner`); after it
/// returns, the SECOND env->funcall must still reach the evaluator context.
unsafe extern "C" fn mod_test_nested_outer(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let bridge = e.intern.unwrap()(env, b"mod-test-nested-bridge\0".as_ptr() as *const _);
    let inner = e.funcall.unwrap()(env, bridge, 0, std::ptr::null_mut());
    // A pending non-local exit means the bridge call itself failed; bail so
    // the test reports that error rather than a follow-on one.
    if !matches!(
        e.non_local_exit_check.unwrap()(env),
        emacs_funcall_exit::Return
    ) {
        return std::ptr::null_mut();
    }
    let plus = e.intern.unwrap()(env, b"+\0".as_ptr() as *const _);
    let mut args = [inner, inner];
    e.funcall.unwrap()(env, plus, 2, args.as_mut_ptr())
}

/// PS-T4 panic-containment driver: run host elisp that panics inside the
/// host evaluator (`neovm--internal-panic`). The panic is contained at the
/// module ABI on the host side and must come back to THIS call as an
/// ordinary pending non-local exit. This module deliberately does not panic
/// itself: it is built with `panic = "abort"`, and a Rust panic cannot cross
/// between two std instances anyway — host-code panics are the containable
/// class.
unsafe extern "C" fn mod_test_panic_host(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let hook = e.intern.unwrap()(env, b"neovm--internal-panic\0".as_ptr() as *const _);
    e.funcall.unwrap()(env, hook, 0, std::ptr::null_mut())
}

unsafe extern "C" fn mod_test_globref_make(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    let val = e.make_integer.unwrap()(env, 42);
    e.make_global_ref.unwrap()(env, val)
}

unsafe extern "C" fn mod_test_globref_free(
    env: *mut emacs_env,
    _nargs: isize,
    args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    let e = env.as_ref().unwrap();
    for i in 0..4 {
        e.free_global_ref.unwrap()(env, *args.add(i as usize));
    }
    e.intern.unwrap()(env, b"t\0".as_ptr() as *const _)
}

// --- emacs_module_init ---

#[unsafe(no_mangle)]
pub unsafe extern "C" fn emacs_module_init(rt: *mut emacs_runtime) -> std::ffi::c_int {
    let e = rt.as_ref().unwrap().get_environment.unwrap()(rt);
    let env = e.as_ref().unwrap();

    // Verify we have the right env size.
    assert!(env.size >= std::mem::size_of::<emacs_env>() as isize - 1024);

    bind_function(
        e,
        "mod-test-return-t",
        env.make_function.unwrap()(
            e,
            0,
            0,
            mod_test_return_t,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-sum",
        env.make_function.unwrap()(
            e,
            2,
            2,
            mod_test_sum,
            b"Return A + B\n\n(fn a b)\0".as_ptr() as *const _,
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-make-string",
        env.make_function.unwrap()(
            e,
            1,
            1,
            mod_test_make_string,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-string-a-to-b",
        env.make_function.unwrap()(
            e,
            1,
            1,
            mod_test_string_a_to_b,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-userptr-make",
        env.make_function.unwrap()(
            e,
            0,
            0,
            mod_test_userptr_make,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-userptr-get",
        env.make_function.unwrap()(
            e,
            1,
            1,
            mod_test_userptr_get,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-vector-fill",
        env.make_function.unwrap()(
            e,
            2,
            2,
            mod_test_vector_fill,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-vector-eq",
        env.make_function.unwrap()(
            e,
            2,
            2,
            mod_test_vector_eq,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-nested-inner",
        env.make_function.unwrap()(
            e,
            0,
            0,
            mod_test_nested_inner,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-nested-outer",
        env.make_function.unwrap()(
            e,
            0,
            0,
            mod_test_nested_outer,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-panic-host",
        env.make_function.unwrap()(
            e,
            0,
            0,
            mod_test_panic_host,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-globref-make",
        env.make_function.unwrap()(
            e,
            0,
            0,
            mod_test_globref_make,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );
    bind_function(
        e,
        "mod-test-globref-free",
        env.make_function.unwrap()(
            e,
            4,
            4,
            mod_test_globref_free,
            std::ptr::null(),
            std::ptr::null_mut(),
        ),
    );

    0
}
