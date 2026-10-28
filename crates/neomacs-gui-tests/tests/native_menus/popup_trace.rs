pub(super) use neomacs_gui_tests::interaction::linux::popup_trace::{ParentRect, configured};

// Lisp readiness precedes native realization during asynchronous startup.
// Track the toplevel role, a committed content buffer, and that surface's
// compositor frame callback, rather than mistaking cursor or sync callbacks
// for permission to send real user input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SurfaceId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CallbackId(u32);

fn object_tail<'a>(text: &'a str, object: &str) -> Option<&'a str> {
    let tail = text.strip_prefix(object)?;
    tail.strip_prefix('#').or_else(|| tail.strip_prefix('@'))
}

fn object_id(text: &str, object: &str) -> Option<u32> {
    object_tail(text, object)?.parse().ok()
}

fn object_request<'a>(line: &'a str, object: &str) -> Option<(u32, &'a str)> {
    let (_, tail) = line.split_once(object)?;
    let tail = tail.strip_prefix('#').or_else(|| tail.strip_prefix('@'))?;
    let (id, request) = tail.split_once('.')?;
    Some((id.parse().ok()?, request))
}

pub(super) fn primary_surface_presented(log: &str) -> bool {
    use std::collections::{HashMap, HashSet};
    let mut roles = HashMap::new();
    let mut primary = None;
    let mut attached = None;
    let mut committed_buffer = false;
    let mut pending_callbacks = Vec::new();
    let mut committed_callbacks = HashSet::new();

    for line in log.lines() {
        if let Some((_, args)) = line.split_once(".get_xdg_surface(new id ")
            && let Some((role, surface)) = args.split_once(", ")
            && let (Some(role), Some(surface)) = (
                object_id(role, "xdg_surface"),
                surface
                    .strip_suffix(')')
                    .and_then(|surface| object_id(surface, "wl_surface")),
            )
        {
            roles.insert(role, SurfaceId(surface));
        }
        if let Some((role, request)) = object_request(line, "xdg_surface")
            && request.starts_with("get_toplevel(")
            && primary.is_none()
        {
            primary = roles.get(&role).copied();
        }
        if let Some((surface, request)) = object_request(line, "wl_surface")
            && Some(SurfaceId(surface)) == primary
        {
            if let Some(args) = request.strip_prefix("attach(") {
                attached = Some(object_tail(args, "wl_buffer").is_some());
            } else if let Some(id) = request
                .strip_prefix("frame(new id ")
                .and_then(|callback| callback.strip_suffix(')'))
                .and_then(|callback| object_id(callback, "wl_callback"))
            {
                pending_callbacks.push(CallbackId(id));
            } else if request == "commit()" {
                if let Some(has_buffer) = attached.take() {
                    committed_buffer = has_buffer;
                }
                if committed_buffer {
                    committed_callbacks.extend(pending_callbacks.drain(..));
                } else {
                    pending_callbacks.clear();
                    committed_callbacks.clear();
                }
            } else if request == "destroy()" {
                primary = None;
                attached = None;
                committed_buffer = false;
                pending_callbacks.clear();
                committed_callbacks.clear();
            }
        }
        if let Some((callback, request)) = object_request(line, "wl_callback")
            && request.starts_with("done(")
            && committed_callbacks.remove(&CallbackId(callback))
            && committed_buffer
        {
            return true;
        }
    }
    false
}

#[test]
fn primary_presentation_excludes_cursor_sync_and_uncommitted_buffers() {
    let role = "[1] -> xdg_wm_base#12.get_xdg_surface(new id xdg_surface#21, wl_surface#20)
                [2] -> xdg_surface#21.get_toplevel(new id xdg_toplevel#22)
";
    let unrelated = "[3] -> wl_surface#16.attach(wl_buffer#30, 0, 0)
                     [4] -> wl_surface#16.frame(new id wl_callback#31)
                     [5] -> wl_surface#16.commit()
                     [6] wl_callback#31.done(100)
                     [7] -> wl_display#1.sync(new id wl_callback#32)
                     [8] wl_callback#32.done(100)
";
    let attached = "[9] -> wl_surface#20.frame(new id wl_callback#33)
                    [10] -> wl_surface#20.attach(wl_buffer#34, 0, 0)
";
    let committed = "[11] -> wl_surface#20.commit()
";
    let done = "[12] wl_callback#33.done(101)
";
    assert!(!primary_surface_presented(&format!("{role}{unrelated}")));
    assert!(!primary_surface_presented(&format!(
        "{role}{unrelated}{attached}"
    )));
    assert!(!primary_surface_presented(&format!(
        "{role}{unrelated}{attached}{committed}"
    )));
    assert!(primary_surface_presented(&format!(
        "{role}{unrelated}{attached}{committed}{done}"
    )));
}

#[test]
fn primary_presentation_rejects_callbacks_before_content_and_after_unmapping() {
    let log = "[1] -> xdg_wm_base#12.get_xdg_surface(new id xdg_surface#21, wl_surface#20)
               [2] -> xdg_surface#21.get_toplevel(new id xdg_toplevel#22)
               [3] -> wl_surface#20.frame(new id wl_callback#30)
               [4] -> wl_surface#20.commit()
               [5] wl_callback#30.done(100)
               [6] -> wl_surface#20.attach(wl_buffer#31, 0, 0)
               [7] -> wl_surface#20.frame(new id wl_callback#32)
               [8] -> wl_surface#20.commit()
               [9] -> wl_surface#20.attach(nil, 0, 0)
               [10] -> wl_surface#20.commit()
               [11] wl_callback#32.done(101)
";
    assert!(!primary_surface_presented(log));
}

#[test]
fn primary_presentation_accepts_ubuntu_wayland_object_syntax() {
    // Actual Ubuntu 24.04 libwayland 1.22 trace from the CI CJK menu case.
    // Unlike newer libwayland's '#', this version separates object IDs by '@'.
    let log = r#"[2191287.949] -> xdg_wm_base@12.get_xdg_surface(new id xdg_surface@21, wl_surface@20)
[2191287.956] -> xdg_surface@21.get_toplevel(new id xdg_toplevel@22)
[2191287.998] -> wl_surface@20.commit()
[2193399.967] -> wl_surface@20.frame(new id wl_callback@46)
[2193519.818] -> wl_surface@20.frame(new id wl_callback@49)
[2193519.836] -> wl_surface@20.attach(wl_buffer@48, 0, 0)
[2193541.624] -> wl_surface@20.commit()
[2193985.403] wl_callback@49.done(224282)
"#;
    assert!(primary_surface_presented(log));
}
