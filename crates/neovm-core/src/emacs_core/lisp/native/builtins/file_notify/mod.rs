use super::*;
use crate::emacs_core::error::expect_args;
use std::cell::RefCell;
use std::path::PathBuf;

#[cfg(any(target_os = "macos", all(test, target_os = "linux")))]
mod kqueue;
#[cfg(not(target_os = "macos"))]
mod notify_rs;
mod subrs;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod kqueue_test;

#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

thread_local! {
    static FILE_NOTIFY_STATE: RefCell<FileNotifyState> = RefCell::new(FileNotifyState::default());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileNotifyWatchDescriptor {
    id: i64,
    generation: i64,
}

impl FileNotifyWatchDescriptor {
    pub(super) fn new(id: i64, generation: i64) -> Self {
        Self { id, generation }
    }

    fn to_lisp(&self) -> Value {
        Value::cons(Value::fixnum(self.id), Value::fixnum(self.generation))
    }

    pub(super) fn id(&self) -> i64 {
        self.id
    }

    pub(super) fn generation(&self) -> i64 {
        self.generation
    }
}

/// Which GNU file-notification surface a watch was created through.
///
/// GNU builds exactly one of `src/inotify.c` and `src/kqueue.c`
/// (`configure.ac' --with-file-notification), so no image holds both kinds at
/// once. Neomacs makes that platform choice at compile time as well: Linux
/// uses the mature `notify` inotify adapter, while macOS retains raw kqueue
/// vnode evidence through `rustix`. The dialect still fixes the Lisp shape:
/// inotify descriptors are conses and events carry a cookie; kqueue
/// descriptors are bare fixnums and events are `(DESCRIPTOR ACTIONS FILE
/// [FILE1])` with kqueue's own action vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WatchDialect {
    Inotify,
    #[cfg(target_os = "macos")]
    Kqueue,
}

/// GNU kqueue's complete Lisp action vocabulary.
///
/// Seven actions correspond to native vnode flags; `create` is synthesized by
/// GNU's directory-list comparison.  Parsing unknown symbols yields `None`
/// because GNU assembles flags with exact `Fmember` probes and ignores the
/// rest.
#[cfg(any(target_os = "macos", test))]
#[enumflags2::bitflags]
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KqueueAction {
    Create = 1 << 0,
    Delete = 1 << 1,
    Write = 1 << 2,
    Extend = 1 << 3,
    Attrib = 1 << 4,
    Link = 1 << 5,
    Rename = 1 << 6,
    Revoke = 1 << 7,
}

#[cfg(any(target_os = "macos", test))]
impl KqueueAction {
    #[cfg(target_os = "macos")]
    fn from_lisp_name(name: &str) -> Option<Self> {
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
    pub(super) const fn as_lisp_name(self) -> &'static str {
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
/// `create' has no NOTE_CREATE bit: GNU synthesizes it by diffing a watched
/// directory after NOTE_WRITE.
#[cfg(any(target_os = "macos", test))]
#[enumflags2::bitflags]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KqueueVnodeAction {
    Delete = 1 << 0,
    Write = 1 << 1,
    Extend = 1 << 2,
    Attrib = 1 << 3,
    Link = 1 << 4,
    Rename = 1 << 5,
    Revoke = 1 << 6,
}

/// Validated request owned by a watch.
///
/// The enum prevents kqueue actions from being interpreted with inotify's
/// aliases and prevents a watch from carrying a dialect that disagrees with
/// its request payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WatchRequest {
    Inotify {
        aspects: Vec<String>,
    },
    #[cfg(target_os = "macos")]
    Kqueue {
        actions: enumflags2::BitFlags<KqueueAction>,
    },
}

impl WatchRequest {
    pub(super) const fn dialect(&self) -> WatchDialect {
        match self {
            Self::Inotify { .. } => WatchDialect::Inotify,
            #[cfg(target_os = "macos")]
            Self::Kqueue { .. } => WatchDialect::Kqueue,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FileWatch {
    pub(super) id: i64,
    pub(super) generation: i64,
    pub(super) path: PathBuf,
    pub(super) is_directory: bool,
    pub(super) callback: Value,
    pub(super) request: WatchRequest,
}

#[derive(Clone, Debug)]
pub(super) enum FileNotifyEvent {
    Inotify {
        descriptor: FileNotifyWatchDescriptor,
        aspects: Vec<&'static str>,
        path: PathBuf,
        cookie: usize,
        callback: Value,
    },
    #[cfg(target_os = "macos")]
    Kqueue {
        descriptor: FileNotifyWatchDescriptor,
        actions: Vec<KqueueAction>,
        path: PathBuf,
        callback: Value,
        /// FILE1 of a `rename' event (src/kqueue.c:171-172).
        file1: Option<PathBuf>,
    },
}

pub(super) trait FileNotifyBackend {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn allocated_p(&self) -> bool;
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn watch_list(&self) -> Vec<FileWatch>;
    fn add_watch(
        &mut self,
        path: &std::path::Path,
        request: WatchRequest,
        callback: Value,
        notifier: Option<crate::emacs_core::process::WaitNotifier>,
    ) -> Result<FileNotifyWatchDescriptor, Flow>;
    fn remove_watch(
        &mut self,
        descriptor: &FileNotifyWatchDescriptor,
        dialect: WatchDialect,
    ) -> Result<bool, Flow>;
    fn valid_p(&self, descriptor: &FileNotifyWatchDescriptor, dialect: WatchDialect) -> bool;
    fn drain_events(&mut self) -> Result<Vec<FileNotifyEvent>, Flow>;
    fn has_watches(&self) -> bool;
}

struct FileNotifyState {
    backend: Box<dyn FileNotifyBackend>,
}

impl Default for FileNotifyState {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let backend: Box<dyn FileNotifyBackend> = Box::<kqueue::KqueueBackend>::default();
        #[cfg(not(target_os = "macos"))]
        let backend: Box<dyn FileNotifyBackend> = Box::<notify_rs::NotifyRsBackend>::default();
        Self { backend }
    }
}

pub(super) fn file_notify_error(
    message: &str,
    detail: Option<String>,
    object: Option<Value>,
) -> Flow {
    let mut tail = match object {
        Some(object) if object.is_cons() => object,
        Some(object) if !object.is_nil() => Value::list(vec![object]),
        _ => Value::NIL,
    };
    if let Some(detail) = detail {
        tail = Value::cons(Value::string(&detail), tail);
    }
    let raw_data = Value::cons(Value::string(message), tail);
    crate::emacs_core::error::signal_with_data("file-notify-error", raw_data)
}

fn inotify_unknown_aspect_error(aspect: Value) -> Flow {
    file_notify_error(
        "Unknown aspect",
        Some("Invalid argument".to_string()),
        Some(aspect),
    )
}

fn inotify_invalid_descriptor_error(descriptor: Value, detail: &str) -> Flow {
    file_notify_error(
        "Invalid descriptor ",
        Some(detail.to_string()),
        Some(descriptor),
    )
}

fn inotify_aspect_symbol_valid(name: &str) -> bool {
    matches!(
        name,
        "access"
            | "attrib"
            | "close-write"
            | "close-nowrite"
            | "create"
            | "delete"
            | "delete-self"
            | "modify"
            | "move-self"
            | "moved-from"
            | "moved-to"
            | "open"
            | "move"
            | "close"
            | "dont-follow"
            | "onlydir"
            | "ignored"
            | "unmount"
            | "all-events"
            | "t"
    )
}

fn validate_inotify_aspect(aspect: Value) -> Result<(), Flow> {
    if aspect.is_nil() {
        return Ok(());
    }
    if let Some(name) = aspect.as_symbol_name() {
        return if inotify_aspect_symbol_valid(name) {
            Ok(())
        } else {
            Err(inotify_unknown_aspect_error(aspect))
        };
    }
    if !aspect.is_cons() {
        return Err(inotify_unknown_aspect_error(aspect));
    }

    let mut rest = aspect;
    while rest.is_cons() {
        let item = rest.cons_car();
        let Some(name) = item.as_symbol_name() else {
            return Err(inotify_unknown_aspect_error(item));
        };
        if !inotify_aspect_symbol_valid(name) {
            return Err(inotify_unknown_aspect_error(item));
        }
        rest = rest.cons_cdr();
    }
    if !rest.is_nil() {
        return Err(inotify_unknown_aspect_error(rest));
    }
    Ok(())
}

fn inotify_aspect_names(aspect: Value) -> Vec<String> {
    if let Some(name) = aspect.as_symbol_name() {
        return vec![name.to_owned()];
    }

    let mut names = Vec::new();
    let mut rest = aspect;
    while rest.is_cons() {
        if let Some(name) = rest.cons_car().as_symbol_name() {
            names.push(name.to_owned());
        }
        rest = rest.cons_cdr();
    }
    names
}

fn extract_valid_watch_descriptor(value: Value) -> Option<FileNotifyWatchDescriptor> {
    if !value.is_cons() {
        return None;
    }
    let id = value.cons_car().as_int()?;
    let generation = value.cons_cdr().as_int()?;
    if id >= 0 && generation >= 0 {
        Some(FileNotifyWatchDescriptor::new(id, generation))
    } else {
        None
    }
}

pub(crate) fn reset_file_notify_thread_locals() {
    FILE_NOTIFY_STATE.with(|slot| *slot.borrow_mut() = FileNotifyState::default());
}

pub(crate) fn collect_file_notify_gc_roots(group: &mut Vec<Value>) {
    FILE_NOTIFY_STATE.with(|slot| {
        group.extend(
            slot.borrow()
                .backend
                .watch_list()
                .into_iter()
                .map(|watch| watch.callback),
        );
    });
}

pub(crate) fn has_active_file_notify_watches() -> bool {
    FILE_NOTIFY_STATE.with(|slot| slot.borrow().backend.has_watches())
}

pub(crate) fn drain_file_notify_events(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<usize, Flow> {
    let events = FILE_NOTIFY_STATE.with(|slot| slot.borrow_mut().backend.drain_events())?;
    let count = events.len();

    for event in events {
        let (raw_event, callback) = match event {
            // GNU inotify events are `(DESCRIPTOR ASPECTS NAME COOKIE)`.
            FileNotifyEvent::Inotify {
                descriptor,
                aspects,
                path,
                cookie,
                callback,
            } => (
                Value::list(vec![
                    descriptor.to_lisp(),
                    Value::list(aspects.into_iter().map(Value::symbol).collect()),
                    Value::string(path.display().to_string()),
                    Value::fixnum(i64::try_from(cookie).unwrap_or(i64::MAX)),
                ]),
                callback,
            ),
            // GNU kqueue events are `(DESCRIPTOR ACTIONS FILE [FILE1])` with
            // a bare-fixnum descriptor and no cookie (`kqueue_generate_event`,
            // src/kqueue.c:94-104).
            #[cfg(target_os = "macos")]
            FileNotifyEvent::Kqueue {
                descriptor,
                actions,
                path,
                callback,
                file1,
            } => {
                let mut fields = vec![
                    Value::fixnum(descriptor.id()),
                    Value::list(
                        actions
                            .into_iter()
                            .map(|action| Value::symbol(action.as_lisp_name()))
                            .collect(),
                    ),
                    Value::string(path.display().to_string()),
                ];
                if let Some(file1) = file1 {
                    fields.push(Value::string(file1.display().to_string()));
                }
                (Value::list(fields), callback)
            }
        };
        ctx.queue_special_event(Value::list(vec![
            Value::symbol("file-notify"),
            raw_event,
            callback,
        ]));
    }

    Ok(count)
}

pub(crate) fn inotify_valid_p(args: Vec<Value>) -> EvalResult {
    expect_args("inotify-valid-p", &args, 1)?;
    let Some(descriptor) = extract_valid_watch_descriptor(args[0]) else {
        return Ok(Value::NIL);
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let state = slot.borrow();
        Ok(Value::bool_val(
            state.backend.valid_p(&descriptor, WatchDialect::Inotify),
        ))
    })
}

pub(crate) fn inotify_add_watch(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("inotify-add-watch", &args, 3)?;
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(ctx.expect_lisp_string(args[0])?);
    validate_inotify_aspect(args[1])?;
    let aspects = inotify_aspect_names(args[1]);
    let callback = args[2];
    let notifier = ctx.wait_notifier();

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let descriptor = state.backend.add_watch(
            &path,
            WatchRequest::Inotify { aspects },
            callback,
            notifier,
        )?;
        Ok(descriptor.to_lisp())
    })
}

pub(crate) fn inotify_rm_watch(args: Vec<Value>) -> EvalResult {
    expect_args("inotify-rm-watch", &args, 1)?;

    let detail = if args[0].is_cons() {
        "Invalid argument"
    } else {
        "No such file or directory"
    };
    let Some(descriptor) = extract_valid_watch_descriptor(args[0]) else {
        return Err(inotify_invalid_descriptor_error(args[0], detail));
    };

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let _ = state
            .backend
            .remove_watch(&descriptor, WatchDialect::Inotify)?;
        Ok(Value::T)
    })
}

/// The kqueue descriptor a Lisp value names, if it could name one.
///
/// GNU kqueue descriptors are bare fixnums -- the open fd
/// (`Fkqueue_add_watch`, src/kqueue.c:460) -- unlike inotify's conses.  This
/// macOS backend returns that owned vnode fd directly, paired internally with
/// generation 0 because GNU's Lisp descriptor has no generation component.
#[cfg(target_os = "macos")]
fn extract_kqueue_watch_descriptor(value: Value) -> Option<FileNotifyWatchDescriptor> {
    let id = value.as_fixnum()?;
    (id >= 0).then(|| FileNotifyWatchDescriptor::new(id, 0))
}

/// GNU `Fkqueue_add_watch` (src/kqueue.c:338): watch FILE for the kqueue
/// actions listed in FLAGS, reporting each through CALLBACK as
/// `(DESCRIPTOR ACTIONS FILE [FILE1])`.
///
/// The checks are GNU's, in GNU's order (:380-389): FILE must be a string
/// naming an existing file (`report_file_error ("File does not exist", ...)`,
/// ENOENT -> `file-missing`); FLAGS must satisfy `CHECK_LIST`; CALLBACK must
/// satisfy `FUNCTIONP` or it is `(wrong-type-argument invalid-function ...)`.
/// A flag symbol kqueue does not know is silently ignored -- the flag
/// assembly is seven native-vnode `Fmember` probes (:440-446), not a
/// validation pass (`create` is generated by directory comparison).
#[cfg(target_os = "macos")]
pub(crate) fn kqueue_add_watch(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kqueue-add-watch", &args, 3)?;
    ctx.expect_lisp_string(args[0])?;
    let expanded =
        crate::emacs_core::fileio::builtin_expand_file_name(ctx, vec![args[0], Value::NIL])?;
    let normalized = crate::emacs_core::fileio::builtin_directory_file_name(ctx, vec![expanded])?;
    if crate::emacs_core::fileio::builtin_file_exists_p(ctx, vec![normalized])?.is_nil() {
        return Err(crate::emacs_core::error::signal(
            "file-missing",
            vec![
                Value::string("File does not exist"),
                Value::string("No such file or directory"),
                normalized,
            ],
        ));
    }
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(ctx.expect_lisp_string(normalized)?);
    let flags = list_to_vec(&args[1]).ok_or_else(|| {
        crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[1]],
        )
    })?;
    if !crate::emacs_core::builtins::value_is_function(ctx, args[2]) {
        return Err(crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("invalid-function"), args[2]],
        ));
    }
    let actions = flags
        .iter()
        .filter_map(|flag| flag.as_symbol_name())
        .filter_map(KqueueAction::from_lisp_name)
        .fold(enumflags2::BitFlags::empty(), |actions, action| {
            actions | action
        });
    let callback = args[2];
    let notifier = ctx.wait_notifier();

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let descriptor =
            state
                .backend
                .add_watch(&path, WatchRequest::Kqueue { actions }, callback, notifier)?;
        Ok(Value::fixnum(descriptor.id()))
    })
}

/// GNU `Fkqueue_rm_watch` (src/kqueue.c:475): unregister the watch and answer
/// t; a descriptor not in the watch list is `(file-notify-error "Not a watch
/// descriptor" WATCH-DESCRIPTOR)`.
#[cfg(target_os = "macos")]
pub(crate) fn kqueue_rm_watch(args: Vec<Value>) -> EvalResult {
    expect_args("kqueue-rm-watch", &args, 1)?;
    let not_a_watch_descriptor =
        || file_notify_error("Not a watch descriptor", None, Some(args[0]));
    let Some(descriptor) = extract_kqueue_watch_descriptor(args[0]) else {
        return Err(not_a_watch_descriptor());
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state
            .backend
            .remove_watch(&descriptor, WatchDialect::Kqueue)?
        {
            Ok(Value::T)
        } else {
            Err(not_a_watch_descriptor())
        }
    })
}

/// GNU `Fkqueue_valid_p` (src/kqueue.c:505): t while the descriptor is in the
/// watch list, nil otherwise; never signals.
#[cfg(target_os = "macos")]
pub(crate) fn kqueue_valid_p(args: Vec<Value>) -> EvalResult {
    expect_args("kqueue-valid-p", &args, 1)?;
    let Some(descriptor) = extract_kqueue_watch_descriptor(args[0]) else {
        return Ok(Value::NIL);
    };
    FILE_NOTIFY_STATE.with(|slot| {
        let state = slot.borrow();
        Ok(Value::bool_val(
            state.backend.valid_p(&descriptor, WatchDialect::Kqueue),
        ))
    })
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
