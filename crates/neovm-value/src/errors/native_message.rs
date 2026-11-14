//! Native `error()` templates have quoting semantics distinct from Lisp data.

/// A native error template, before GNU `doprnt` quote substitution.
/// This immutable borrow is call-local and contains no mutator state.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct CErrorMessage<'a> {
    template: &'a str,
    string_arg: Option<&'a str>,
}

static_assertions::assert_impl_all!(CErrorMessage<'static>: Send, Sync);

impl<'a> CErrorMessage<'a> {
    /// The raw GNU template, before quote substitution.
    #[doc(hidden)]
    pub fn template(self) -> &'a str {
        self.template
    }

    /// TEMPLATE must contain exactly one literal `%s` placeholder, with no
    /// printf flags, widths, or other substitutions. ARGUMENT is literal data:
    /// substitute it once after requoting TEMPLATE, preserving its quotes,
    /// percent sequences, and embedded NUL bytes.
    #[doc(hidden)]
    pub fn with_string_arg(template: &'a str, argument: &'a str) -> Self {
        Self {
            template,
            string_arg: Some(argument),
        }
    }

    #[doc(hidden)]
    pub fn string_arg(self) -> Option<&'a str> {
        self.string_arg
    }
}

impl<'a> From<&'a str> for CErrorMessage<'a> {
    fn from(template: &'a str) -> Self {
        Self {
            template,
            string_arg: None,
        }
    }
}
