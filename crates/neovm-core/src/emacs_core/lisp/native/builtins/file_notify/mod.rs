use super::*;
use crate::emacs_core::error::expect_args;
use std::cell::RefCell;
use std::path::PathBuf;

mod notify_rs;

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
/// (`configure.ac' --with-file-notification), so no GNU image ever holds
/// both kinds at once; this port's single `notify`-crate backend serves
/// whichever surface the platform advertises, and the dialect decides the
/// Lisp shape of everything the watch produces: inotify descriptors are
/// conses and events carry a trailing cookie, kqueue descriptors are bare
/// fixnums (the fd in GNU) and events are `(DESCRIPTOR ACTIONS FILE
/// [FILE1])` with kqueue's own action vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WatchDialect {
    Inotify,
    Kqueue,
}

#[derive(Clone, Debug)]
pub(super) struct FileWatch {
    pub(super) id: i64,
    pub(super) generation: i64,
    pub(super) path: PathBuf,
    pub(super) is_directory: bool,
    pub(super) aspects: Vec<String>,
    pub(super) callback: Value,
    pub(super) dialect: WatchDialect,
}

#[derive(Clone, Debug)]
pub(super) struct FileNotifyEvent {
    pub(super) descriptor: FileNotifyWatchDescriptor,
    pub(super) aspects: Vec<&'static str>,
    pub(super) path: PathBuf,
    pub(super) cookie: usize,
    pub(super) callback: Value,
    pub(super) dialect: WatchDialect,
    /// kqueue only: FILE1 of a `rename' event (src/kqueue.c:171-172).
    pub(super) file1: Option<PathBuf>,
}

pub(super) trait FileNotifyBackend {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn allocated_p(&self) -> bool;
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn watch_list(&self) -> Vec<FileWatch>;
    fn add_watch(
        &mut self,
        path: &std::path::Path,
        aspects: Vec<String>,
        callback: Value,
        notifier: Option<crate::emacs_core::process::WaitNotifier>,
        dialect: WatchDialect,
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
        Self {
            backend: Box::<notify_rs::NotifyRsInotifyBackend>::default(),
        }
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
        let raw_event = match event.dialect {
            // GNU inotify events are `(DESCRIPTOR ASPECTS NAME COOKIE)`.
            WatchDialect::Inotify => Value::list(vec![
                event.descriptor.to_lisp(),
                Value::list(event.aspects.into_iter().map(Value::symbol).collect()),
                Value::string(event.path.display().to_string()),
                Value::fixnum(i64::try_from(event.cookie).unwrap_or(i64::MAX)),
            ]),
            // GNU kqueue events are `(DESCRIPTOR ACTIONS FILE [FILE1])` with
            // a bare-fixnum descriptor and no cookie (`kqueue_generate_event`,
            // src/kqueue.c:94-104).
            WatchDialect::Kqueue => {
                let mut fields = vec![
                    Value::fixnum(event.descriptor.id()),
                    Value::list(event.aspects.into_iter().map(Value::symbol).collect()),
                    Value::string(event.path.display().to_string()),
                ];
                if let Some(file1) = event.file1 {
                    fields.push(Value::string(file1.display().to_string()));
                }
                Value::list(fields)
            }
        };
        ctx.queue_special_event(Value::list(vec![
            Value::symbol("file-notify"),
            raw_event,
            event.callback,
        ]));
    }

    Ok(count)
}

pub(crate) fn builtin_inotify_valid_p(args: Vec<Value>) -> EvalResult {
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

pub(crate) fn builtin_inotify_add_watch(
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
        let descriptor =
            state
                .backend
                .add_watch(&path, aspects, callback, notifier, WatchDialect::Inotify)?;
        Ok(descriptor.to_lisp())
    })
}

pub(crate) fn builtin_inotify_rm_watch(args: Vec<Value>) -> EvalResult {
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
/// port has no fd, so the fixnum is the watch id, paired with generation 0.
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
/// assembly is eight `Fmember` probes (:440-446), not a validation pass.
pub(crate) fn builtin_kqueue_add_watch(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kqueue-add-watch", &args, 3)?;
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(ctx.expect_lisp_string(args[0])?);
    if !path.exists() {
        return Err(crate::emacs_core::error::signal(
            "file-missing",
            vec![
                Value::string("File does not exist"),
                Value::string("No such file or directory"),
                args[0],
            ],
        ));
    }
    if !(args[1].is_nil() || args[1].is_cons()) {
        return Err(crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[1]],
        ));
    }
    if !crate::emacs_core::builtins::value_is_function(ctx, args[2]) {
        return Err(crate::emacs_core::error::signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("invalid-function"), args[2]],
        ));
    }
    let flags = inotify_aspect_names(args[1]);
    let callback = args[2];
    let notifier = ctx.wait_notifier();

    FILE_NOTIFY_STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let descriptor =
            state
                .backend
                .add_watch(&path, flags, callback, notifier, WatchDialect::Kqueue)?;
        Ok(Value::fixnum(descriptor.id()))
    })
}

/// GNU `Fkqueue_rm_watch` (src/kqueue.c:475): unregister the watch and answer
/// t; a descriptor not in the watch list is `(file-notify-error "Not a watch
/// descriptor" WATCH-DESCRIPTOR)`.
pub(crate) fn builtin_kqueue_rm_watch(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_kqueue_valid_p(args: Vec<Value>) -> EvalResult {
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
