//! GNU file-attributes representation, independent of the storage adapter.

use crate::emacs_core::eval::Context;
use crate::emacs_core::fileio;
use crate::emacs_core::timefns::{LispTimeOutput, make_lisp_time};
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
pub(super) enum FileIdFormat {
    #[strum(serialize = "integer")]
    Integer,
    #[strum(serialize = "string")]
    String,
}

impl FileIdFormat {
    pub(super) fn from_id_format_arg(arg: Option<&Value>) -> Self {
        let Some(value) = arg else {
            return Self::Integer;
        };
        if value.is_nil() {
            return Self::Integer;
        }
        value
            .as_symbol_name()
            .and_then(|name| name.parse::<Self>().ok())
            .unwrap_or(Self::String)
    }

    fn ids_as_strings(self) -> bool {
        matches!(self, Self::String)
    }
}

/// Build the Emacs-compatible file-attributes list for a path.
///
/// Returns:
///   (TYPE NLINKS UID GID ATIME MTIME CTIME SIZE MODE GID-CHANGEP INODE DEVICE)
///
/// TYPE is:
///   t        for a directory
///   nil      for a regular file
///   string   for a symlink (the link target)
///
/// Times use the representation selected by `time_output`.
/// If ID-FORMAT requests strings, resolved names are returned; GNU falls back
/// to numeric IDs when name lookup fails.
/// Virtual stores can lack POSIX fields: unknown fields are nil, except that
/// an unknown mtime uses GNU's explicit visited-file-modtime flag 0. This
/// prevents dired-readin from treating nil as a request to stat its non-file
/// buffer. Unknown permission bits are `?`. A missing entry returns nil for the whole
/// result. In particular, absent identity must not become a shared zero inode.
pub(super) fn build_file_attributes(
    eval: &Context,
    filename: &LispString,
    id_format: FileIdFormat,
    time_output: LispTimeOutput,
) -> Option<Value> {
    use fileio::{FileAttributeType, FilePrincipal, FileTimestamp};
    let path = fileio::lisp_file_name_to_path_buf(filename);
    let attributes = eval.editor_file_system().attributes(&path).ok()?;
    let file_type = match &attributes.kind {
        FileAttributeType::Directory => Value::T,
        FileAttributeType::SymbolicLink(target) => {
            Value::heap_string(fileio::path_to_lisp_file_name(target))
        }
        FileAttributeType::Other => Value::NIL,
    };
    let principal = |owner: Option<FilePrincipal>| match owner {
        Some(FilePrincipal {
            name: Some(name), ..
        }) if id_format.ids_as_strings() => Value::string(name),
        Some(owner) => Value::make_integer(owner.id.into()),
        None => Value::NIL,
    };
    let timestamp = |time: Option<FileTimestamp>| {
        time.map(|time| make_lisp_time(time.seconds, i64::from(time.nanoseconds), time_output))
            .unwrap_or(Value::NIL)
    };
    let integer = |number: Option<u64>| {
        number
            .map(|number| Value::make_integer(number.into()))
            .unwrap_or(Value::NIL)
    };
    let mode = match attributes.mode {
        Some(mode) => format_attribute_mode(mode.bits(), &attributes.kind),
        None => format!(
            "{}?????????",
            match attributes.kind {
                FileAttributeType::Directory => 'd',
                FileAttributeType::SymbolicLink(_) => 'l',
                FileAttributeType::Other => '-',
            }
        ),
    };
    Some(Value::list(vec![
        file_type,
        integer(attributes.links),
        principal(attributes.user),
        principal(attributes.group),
        timestamp(attributes.accessed),
        attributes.modified.map_or_else(
            || crate::buffer::VisitedFileModtime::Unknown.to_lisp_value(),
            |time| timestamp(Some(time)),
        ),
        timestamp(attributes.changed),
        Value::make_integer(attributes.len.into()),
        Value::string(mode),
        Value::bool(attributes.legacy_group_change),
        integer(attributes.identity.map(|id| id.inode)),
        integer(attributes.identity.map(|id| id.device)),
    ]))
}

/// Format a Unix file mode string like "drwxr-xr-x" or "-rw-r--r--".
pub(super) fn format_attribute_mode(mode: u32, kind: &fileio::FileAttributeType) -> String {
    let mut s = String::with_capacity(10);

    // File type character.
    if matches!(kind, fileio::FileAttributeType::SymbolicLink(_)) {
        s.push('l');
    } else if matches!(kind, fileio::FileAttributeType::Directory) {
        s.push('d');
    } else {
        s.push('-');
    }

    // Owner permissions.
    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o4000 != 0 {
        if mode & 0o100 != 0 { 's' } else { 'S' }
    } else if mode & 0o100 != 0 {
        'x'
    } else {
        '-'
    });

    // Group permissions.
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o2000 != 0 {
        if mode & 0o010 != 0 { 's' } else { 'S' }
    } else if mode & 0o010 != 0 {
        'x'
    } else {
        '-'
    });

    // Other permissions.
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o1000 != 0 {
        if mode & 0o001 != 0 { 't' } else { 'T' }
    } else if mode & 0o001 != 0 {
        'x'
    } else {
        '-'
    });

    s
}
