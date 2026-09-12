//! Desktop font preferences are requests, not realized fonts or frame state.

/// Desktop applications distinguish document fixed-width and normal UI fonts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumIter)]
pub enum SystemFontRole {
    Monospace,
    Application,
}

/// An owned native font description. Its size/style syntax is interpreted by
/// the same GNU-compatible parser used for explicit Lisp font requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemFontName(String);

impl SystemFontName {
    pub fn new(name: String) -> Option<Self> {
        (!name.trim().is_empty() && !name.contains('\0')).then_some(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One coherent discovery result, safe to transfer between native startup and
/// the evaluator. Absent preferences preserve the existing platform fallback.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemFonts {
    monospace: Option<SystemFontName>,
    application: Option<SystemFontName>,
}

impl SystemFonts {
    pub fn new(monospace: Option<SystemFontName>, application: Option<SystemFontName>) -> Self {
        Self {
            monospace,
            application,
        }
    }

    pub fn get(&self, role: SystemFontRole) -> Option<&SystemFontName> {
        match role {
            SystemFontRole::Monospace => self.monospace.as_ref(),
            SystemFontRole::Application => self.application.as_ref(),
        }
    }
}
