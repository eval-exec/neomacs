//! Native Lisp declarations owned by GNU `src/dbusbind.c`'s `syms_of_dbusbind`.

use crate::emacs_core::eval::Context;

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

        pub(super) fn install_lisp_state(ctx: &mut Context) {
            use crate::emacs_core::value::{HashTableTest, Value};

            let compiled = option_env!("NEOMACS_DBUS_COMPILED_VERSION")
                .map(Value::string)
                .unwrap_or(Value::NIL);
            ctx.obarray
                .define_special_variable("dbus-compiled-version", compiled);
            ctx.obarray
                .define_special_variable("dbus-runtime-version", compiled);
            ctx.obarray
                .define_special_variable("dbus-message-type-invalid", Value::fixnum(0));
            ctx.obarray
                .define_special_variable("dbus-message-type-method-call", Value::fixnum(1));
            ctx.obarray
                .define_special_variable("dbus-message-type-method-return", Value::fixnum(2));
            ctx.obarray
                .define_special_variable("dbus-message-type-error", Value::fixnum(3));
            ctx.obarray
                .define_special_variable("dbus-message-type-signal", Value::fixnum(4));
            ctx.obarray.define_special_variable(
                "dbus-registered-objects-table",
                Value::hash_table(HashTableTest::Equal),
            );
            ctx.obarray
                .define_special_variable("dbus-debug", Value::NIL);

            crate::emacs_core::errors::register_dbus_error(&mut ctx.obarray);
        }
    }
    _ => {
        pub(super) fn register_subrs(ctx: &mut Context) {
            let _ = ctx;
        }
    }
}
