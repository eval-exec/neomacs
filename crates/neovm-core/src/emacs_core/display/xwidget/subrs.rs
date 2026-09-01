//! Native Lisp declarations for the xwidget runtime.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "make-xwidget",
        NativeFn::ContextVec(create),
        SubrArity::new(4, Some(7)),
    ),
    SubrSpec::new(
        "xwidgetp",
        NativeFn::ContextVec(is_xwidget),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-view-p",
        NativeFn::ContextVec(is_view),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-live-p",
        NativeFn::ContextVec(is_live),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-info",
        NativeFn::ContextVec(info),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-view-info",
        NativeFn::ContextVec(view_info),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-view-model",
        NativeFn::ContextVec(view_model),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-view-window",
        NativeFn::ContextVec(view_window),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-view-lookup",
        NativeFn::ContextVec(lookup_view),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "delete-xwidget-view",
        NativeFn::ContextVec(delete_view),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-plist",
        NativeFn::ContextVec(plist),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "set-xwidget-plist",
        NativeFn::ContextVec(set_plist),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "xwidget-buffer",
        NativeFn::ContextVec(buffer),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "set-xwidget-buffer",
        NativeFn::ContextVec(set_buffer),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "xwidget-query-on-exit-flag",
        NativeFn::ContextVec(query_on_exit),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "set-xwidget-query-on-exit-flag",
        NativeFn::ContextVec(set_query_on_exit),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "get-buffer-xwidgets",
        NativeFn::ContextVec(buffer_xwidgets),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "kill-xwidget",
        NativeFn::ContextVec(kill),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-resize",
        NativeFn::ContextVec(resize),
        SubrArity::new(3, Some(3)),
    ),
    SubrSpec::new(
        "xwidget-size-request",
        NativeFn::ContextVec(size_request),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-webkit-uri",
        NativeFn::ContextVec(webkit_uri),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-webkit-title",
        NativeFn::ContextVec(webkit_title),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "xwidget-webkit-goto-uri",
        NativeFn::ContextVec(navigate_webkit),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "xwidget-webkit-execute-script",
        NativeFn::ContextVec(execute_script),
        SubrArity::new(2, Some(3)),
    ),
    SubrSpec::new(
        "xwidget-webkit-estimated-load-progress",
        NativeFn::ContextVec(estimated_load_progress),
        SubrArity::new(1, Some(1)),
    ),
}
