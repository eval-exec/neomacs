//! Native `error()` templates have quoting semantics distinct from Lisp data.

/// A native error template, before GNU `doprnt` quote substitution.
/// This immutable borrow is call-local and contains no mutator state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CErrorMessage<'a>(&'a str);

static_assertions::assert_impl_all!(CErrorMessage<'static>: Send, Sync);

impl<'a> CErrorMessage<'a> {
    /// The raw GNU template, before quote substitution.
    pub(crate) fn template(self) -> &'a str {
        self.0
    }
}

impl<'a> From<&'a str> for CErrorMessage<'a> {
    fn from(template: &'a str) -> Self {
        Self(template)
    }
}
