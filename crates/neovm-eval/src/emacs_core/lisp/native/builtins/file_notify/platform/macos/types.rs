//! Typed kqueue request and native-event vocabularies.

/// GNU kqueue's complete Lisp action vocabulary.
///
/// Seven actions correspond to native vnode flags; `create` is synthesized by
/// GNU's directory-list comparison. Parsing unknown symbols yields `None`
/// because GNU assembles flags with exact `Fmember` probes and ignores the
/// rest.
#[enumflags2::bitflags]
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super::super) enum KqueueAction {
    Create = 1 << 0,
    Delete = 1 << 1,
    Write = 1 << 2,
    Extend = 1 << 3,
    Attrib = 1 << 4,
    Link = 1 << 5,
    Rename = 1 << 6,
    Revoke = 1 << 7,
}

impl KqueueAction {
    #[cfg(target_os = "macos")]
    #[doc(hidden)]
    pub fn from_lisp_name(name: &str) -> Option<Self> {
        match name {
            "create" => Some(Self::Create),
            "delete" => Some(Self::Delete),
            "write" => Some(Self::Write),
            "extend" => Some(Self::Extend),
            "attrib" => Some(Self::Attrib),
            "link" => Some(Self::Link),
            "rename" => Some(Self::Rename),
            "revoke" => Some(Self::Revoke),
            _ => None,
        }
    }

    #[cfg(target_os = "macos")]
    #[doc(hidden)]
    pub const fn as_lisp_name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Delete => "delete",
            Self::Write => "write",
            Self::Extend => "extend",
            Self::Attrib => "attrib",
            Self::Link => "link",
            Self::Rename => "rename",
            Self::Revoke => "revoke",
        }
    }
}

/// Native vnode evidence, kept distinct from the Lisp action set because
/// `create` has no `NOTE_CREATE` bit: GNU synthesizes it by diffing a watched
/// directory after `NOTE_WRITE`.
#[enumflags2::bitflags]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super::super) enum KqueueVnodeAction {
    Delete = 1 << 0,
    Write = 1 << 1,
    Extend = 1 << 2,
    Attrib = 1 << 3,
    Link = 1 << 4,
    Rename = 1 << 5,
    Revoke = 1 << 6,
}
