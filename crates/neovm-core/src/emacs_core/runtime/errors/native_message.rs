//! Native `error()` templates have quoting semantics distinct from Lisp data.

use crate::emacs_core::{
    coding,
    error::{Flow, LispCondition},
    eval::Context,
    value::Value,
};

/// A native error template, before GNU `doprnt` quote substitution.
/// This immutable borrow is call-local and contains no mutator state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CErrorMessage<'a>(&'a str);

static_assertions::assert_impl_all!(CErrorMessage<'static>: Send, Sync);

impl<'a> From<&'a str> for CErrorMessage<'a> {
    fn from(template: &'a str) -> Self {
        Self(template)
    }
}

impl Context {
    /// GNU eval.c:2285-2287 routes native error templates through doprnt;
    /// doprnt.c:493-499 translates quotes according to text-quoting-style.
    #[cold]
    #[inline(never)]
    pub(crate) fn signal_c_error(&self, message: CErrorMessage<'_>) -> Flow {
        let style = coding::effective_text_quoting_style(&self.obarray);
        crate::emacs_core::error::signal(
            LispCondition::Error,
            vec![Value::string(coding::requote_c_error_message(
                message.0, style,
            ))],
        )
    }
}
