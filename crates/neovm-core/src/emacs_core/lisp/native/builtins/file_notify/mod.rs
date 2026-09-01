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

#[derive(Clone, Debug)]
pub(super) struct FileWatch {
    pub(super) id: i64,
    pub(super) generation: i64,
    pub(super) path: PathBuf,
    pub(super) is_directory: bool,
    pub(super) aspects: Vec<String>,
    pub(super) callback: Value,
}

#[derive(Clone, Debug)]
pub(super) struct FileNotifyEvent {
    pub(super) descriptor: FileNotifyWatchDescriptor,
    pub(super) aspects: Vec<&'static str>,
    pub(super) path: PathBuf,
    pub(super) cookie: usize,
    pub(super) callback: Value,
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
    ) -> Result<FileNotifyWatchDescriptor, Flow>;
    fn remove_watch(&mut self, descriptor: &FileNotifyWatchDescriptor) -> Result<bool, Flow>;
    fn valid_p(&self, descriptor: &FileNotifyWatchDescriptor) -> bool;
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
        let raw_event = Value::list(vec![
            event.descriptor.to_lisp(),
            Value::list(event.aspects.into_iter().map(Value::symbol).collect()),
            Value::string(event.path.display().to_string()),
            Value::fixnum(i64::try_from(event.cookie).unwrap_or(i64::MAX)),
        ]);
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
        Ok(Value::bool_val(state.backend.valid_p(&descriptor)))
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
        let descriptor = state
            .backend
            .add_watch(&path, aspects, callback, notifier)?;
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
        let _ = state.backend.remove_watch(&descriptor)?;
        Ok(Value::T)
    })
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
