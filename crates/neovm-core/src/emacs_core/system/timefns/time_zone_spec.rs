/// A borrowed POSIX timezone specification with GNU's C-string termination.
///
/// The private field contains no NUL, so environment mutation cannot panic on
/// Lisp-provided bytes. This immutable borrowed value carries no mutator state
/// and can be shared between threads; environment guards serialize changes.
#[derive(Clone, Copy, Debug)]
pub(super) struct TimeZoneSpec<'a>(&'a str);

impl<'a> From<&'a str> for TimeZoneSpec<'a> {
    fn from(spec: &'a str) -> Self {
        Self(spec.split_once('\0').map_or(spec, |(prefix, _)| prefix))
    }
}

impl AsRef<str> for TimeZoneSpec<'_> {
    fn as_ref(&self) -> &str {
        self.0
    }
}

static_assertions::assert_impl_all!(TimeZoneSpec<'static>: Send, Sync);
