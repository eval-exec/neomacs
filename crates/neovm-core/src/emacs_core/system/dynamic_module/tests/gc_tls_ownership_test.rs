use super::*;

struct Environment {
    env: Box<emacs_env>,
    private: Box<emacs_env_private>,
}

impl Environment {
    fn new() -> Self {
        let mut private = Box::new(emacs_env_private {
            pending_non_local_exit: emacs_funcall_exit::Return,
            non_local_exit_symbol: Value::NIL,
            non_local_exit_data: Value::NIL,
            storage: emacs_value_storage::new(),
        });
        let mut env = Box::new(unsafe { std::mem::zeroed::<emacs_env>() });
        unsafe { initialize_environment(&mut *env, &mut *private) };
        Self { env, private }
    }

    fn global_ref(&mut self, value: Value) -> GlobalReference {
        let local = lisp_to_value(&mut *self.env, value);
        let reference = unsafe { module_make_global_ref(&mut *self.env, local) };
        assert!(!reference.is_null());
        GlobalReference(reference)
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        unsafe { finalize_storage(&mut self.private.storage) };
    }
}

struct GlobalReference(emacs_value);

impl Drop for GlobalReference {
    fn drop(&mut self) {
        unsafe { module_free_global_ref(std::ptr::null_mut(), self.0) };
    }
}

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_dynamic_module_registry_gc_roots(&ctx.dynamic_module_registry, &mut roots);
    collect_dynamic_module_gc_roots(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_module_global_refs_exclude_a_dropped_heap() {
    let (old, reference) = {
        let ctx = Context::new();
        let mut env = Environment::new();
        let value = Value::vector(vec![Value::fixnum(42)]);
        let reference = env.global_ref(value);
        assert!(
            roots_for(&ctx)
                .iter()
                .any(|root| root.bits() == value.bits())
        );
        (value, reference)
    };
    assert!(value_to_lisp(reference.0).is_nil());
    let mut next = Context::new();
    assert!(
        !roots_for(&next)
            .iter()
            .any(|root| root.bits() == old.bits()),
        "a module global reference outlived its Context heap"
    );
    next.gc_collect_exact();
    drop(reference);
}

#[test]
fn gc_tls_ownership_module_global_refs_preserve_each_live_heap() {
    let mut first = Context::new();
    let mut env = Environment::new();
    let a = Value::vector(vec![Value::fixnum(1)]);
    let a_ref = env.global_ref(a);
    let mut second = Context::new();
    let b = Value::vector(vec![Value::fixnum(2)]);
    let b_ref = env.global_ref(b);
    assert!(
        roots_for(&second)
            .iter()
            .all(|root| second.tagged_heap.owns_heap_value_for_test(*root)),
        "module global references from the first Context are roots of the second heap"
    );
    second.gc_collect_exact();
    first.setup_thread_locals();
    assert!(roots_for(&first).iter().any(|root| root.bits() == a.bits()));
    assert!(!roots_for(&first).iter().any(|root| root.bits() == b.bits()));
    first.gc_collect_exact();
    assert_eq!(a.as_vector_data().unwrap()[0], Value::fixnum(1));
    assert_eq!(b.as_vector_data().unwrap()[0], Value::fixnum(2));
    drop((a_ref, b_ref));
    assert!(roots_for(&first).is_empty());
    assert!(roots_for(&second).is_empty());
}

#[test]
fn gc_tls_ownership_module_active_environment_excludes_another_live_heap() {
    let mut first = Context::new();
    let mut env = Environment::new();
    let value = Value::vector(vec![Value::fixnum(42)]);
    lisp_to_value(&mut *env.env, value);
    env.private.non_local_exit_data = Value::list(vec![value]);
    let active = ActiveModuleEnv::push(&mut *env.private);
    assert!(
        roots_for(&first)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    let mut second = Context::new();
    assert!(
        roots_for(&second)
            .iter()
            .all(|root| second.tagged_heap.owns_heap_value_for_test(*root)),
        "an active module arena belongs to the first heap"
    );
    second.gc_collect_exact();
    first.setup_thread_locals();
    first.gc_collect_exact();
    assert_eq!(value.as_vector_data().unwrap()[0], Value::fixnum(42));
    drop(active);
}

#[test]
fn gc_tls_ownership_module_overflow_arena_values_are_gc_roots() {
    let mut ctx = Context::new();
    let mut env = Environment::new();
    let mut last = Value::NIL;
    for index in 0..(VALUE_FRAME_SIZE + 1) {
        last = Value::vector(vec![Value::fixnum(index as i64)]);
        lisp_to_value(&mut *env.env, last);
    }
    let active = ActiveModuleEnv::push(&mut *env.private);
    assert!(
        roots_for(&ctx)
            .iter()
            .any(|root| root.bits() == last.bits()),
        "active module root traversal skipped an overflow value-arena frame"
    );
    ctx.gc_collect_exact();
    assert_eq!(
        last.as_vector_data().unwrap()[0],
        Value::fixnum(VALUE_FRAME_SIZE as i64)
    );
    drop(active);
}

#[test]
fn gc_tls_ownership_module_global_refs_keep_equal_but_distinct_objects() {
    let mut ctx = Context::new();
    let mut env = Environment::new();
    let a = Value::vector(vec![Value::fixnum(42)]);
    let b = Value::vector(vec![Value::fixnum(42)]);
    assert_ne!(a.bits(), b.bits());
    let a_ref = env.global_ref(a);
    let b_ref = env.global_ref(b);
    assert_ne!(
        a_ref.0, b_ref.0,
        "module global references collapsed equal but distinct Lisp objects"
    );
    assert_eq!(value_to_lisp(a_ref.0).bits(), a.bits());
    assert_eq!(value_to_lisp(b_ref.0).bits(), b.bits());
    ctx.gc_collect_exact();
    assert_eq!(a.as_vector_data().unwrap()[0], Value::fixnum(42));
    assert_eq!(b.as_vector_data().unwrap()[0], Value::fixnum(42));
    drop((a_ref, b_ref));
}

#[test]
fn gc_tls_ownership_module_global_refs_follow_context_migration() {
    let mut first = Context::new();
    let mut env = Environment::new();
    let value = Value::vector(vec![Value::fixnum(42)]);
    let reference = env.global_ref(value);
    drop(env);
    let (ready, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        receive.recv().unwrap();
        first.setup_thread_locals();
        assert!(
            roots_for(&first)
                .iter()
                .any(|root| root.bits() == value.bits()),
            "module global references stayed on the source thread after their Context moved"
        );
        first.gc_collect_exact();
        assert_eq!(value.as_vector_data().unwrap()[0], Value::fixnum(42));
    });
    // Retire the source thread's active aliases before the worker mutates A.
    let mut second = Context::new();
    ready.send(()).unwrap();
    worker.join().expect("collect the migrated Context");
    assert!(value_to_lisp(reference.0).is_nil());
    second.gc_collect_exact();
    drop(reference);
}
