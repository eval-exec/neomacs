//! Native Lisp declarations owned by GNU `src/dbusbind.c`'s `syms_of_dbusbind`.

std::cfg_select! {
    neomacs_have_dbus => {
        use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

        crate::emacs_core::subr::define_subrs! {
            SubrSpec::new(
                "dbus--init-bus",
                NativeFn::ContextVec(super::connection::init_bus),
                SubrArity::new(1, Some(2)),
            ),
            SubrSpec::new(
                "dbus-get-unique-name",
                NativeFn::NoContextVec(super::connection::get_unique_name),
                SubrArity::new(1, Some(1)),
            ),
            SubrSpec::new(
                "dbus-message-internal",
                NativeFn::ContextVec(super::message::message_internal),
                SubrArity::new(3, None),
            ),
            SubrSpec::new(
                "dbus--fd-open",
                NativeFn::ContextVec(super::fd::fd_open),
                SubrArity::new(1, Some(1)),
            ),
            SubrSpec::new(
                "dbus--fd-close",
                NativeFn::NoContextVec(super::fd::fd_close),
                SubrArity::new(1, Some(1)),
            ),
            SubrSpec::new(
                "dbus--registered-fds",
                NativeFn::NoContextVec(super::fd::registered_fds),
                SubrArity::new(0, Some(0)),
            ),
        }

        /// The `DEFVAR`s and the `dbus-error` symbol of GNU's
        /// `syms_of_dbusbind`.  Called with the other subsystems'
        /// `register_bootstrap_vars` while the evaluator is being built, so
        /// `defvar_object::adopt` gives the nine names GNU's forwarded storage
        /// like every other C variable; a pdump-restored evaluator carries
        /// them in its image, as GNU's does.
        pub(super) fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
            use crate::emacs_core::value::{HashTableTest, Value};

            let compiled = option_env!("NEOMACS_DBUS_COMPILED_VERSION")
                .map(Value::string)
                .unwrap_or(Value::NIL);
            obarray.define_special_variable("dbus-compiled-version", compiled);
            obarray.define_special_variable("dbus-runtime-version", compiled);
            obarray.define_special_variable("dbus-message-type-invalid", Value::fixnum(0));
            obarray.define_special_variable("dbus-message-type-method-call", Value::fixnum(1));
            obarray.define_special_variable("dbus-message-type-method-return", Value::fixnum(2));
            obarray.define_special_variable("dbus-message-type-error", Value::fixnum(3));
            obarray.define_special_variable("dbus-message-type-signal", Value::fixnum(4));
            obarray.define_special_variable(
                "dbus-registered-objects-table",
                Value::hash_table(HashTableTest::Equal),
            );
            obarray.define_special_variable("dbus-debug", Value::NIL);

            crate::emacs_core::errors::register_dbus_error(obarray);
        }
    }
    _ => {
        pub(super) fn register_subrs(ctx: &mut crate::emacs_core::eval::Context) {
            let _ = ctx;
        }
    }
}
