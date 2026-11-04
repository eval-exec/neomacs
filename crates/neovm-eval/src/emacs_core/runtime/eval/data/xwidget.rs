//! Xwidget runtime state owned by the evaluator layer (stage-4 W2 E5).
//!
//! GNU Emacs stores xwidgets and xwidget views as heap pseudovectors; the
//! evaluator owns the internal lists, the per-view WebKit runtime facts and
//! the pending asynchronous script callbacks. The display-side event
//! interpreter stays in `emacs_core::display::xwidget` and drives this state
//! through the fields and accessors below (`XwidgetStateHost`).

use std::collections::HashMap;

use crate::emacs_core::eval::VariableStore;
use neomacs_display_protocol::{WebViewId, XwidgetId};
use neovm_value::display_vocab::XwidgetScriptRequestId;
use neovm_value::value::Value;

#[derive(Clone, Debug, Default)]
pub struct WebKitRuntimeState {
    pub generation: u64,
    pub uri: String,
    pub title: String,
    /// Estimated load progress, 0.0..=1.0, as `xwidget-webkit-estimated-load-
    /// progress' reports it.
    ///
    /// GNU reads WebKitGTK's continuous `estimated-load-progress' property.
    /// Neomacs resets this to 0.0 when dispatching a navigation, then
    /// applies generation-qualified measurements from backends that expose
    /// progress (WPEPlatform, and KVO on WKWebView's `estimatedProgress'),
    /// and pins 1.0 at "load-finished".
    pub load_progress: f64,
}

#[derive(Clone, Debug)]
pub struct PendingScriptCallback {
    pub view: WebViewId,
    pub function: Value,
}

#[derive(Clone, Debug)]
pub struct XwidgetState {
    pub internal_xwidget_list: Value,
    pub internal_xwidget_view_list: Value,
    pub webkit_state: HashMap<WebViewId, WebKitRuntimeState>,
    pub script_callbacks: HashMap<XwidgetScriptRequestId, PendingScriptCallback>,
    pub xwidget_counter: u32,
    pub webview_counter: u32,
    pub script_counter: u64,
}

impl XwidgetState {
    pub fn new() -> Self {
        Self {
            internal_xwidget_list: Value::NIL,
            internal_xwidget_view_list: Value::NIL,
            webkit_state: HashMap::new(),
            script_callbacks: HashMap::new(),
            xwidget_counter: 0,
            webview_counter: 0,
            script_counter: 0,
        }
    }

    pub fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        visit(self.internal_xwidget_list);
        visit(self.internal_xwidget_view_list);
        for callback in self.script_callbacks.values() {
            visit(callback.function);
        }
    }

    pub fn next_ids(&mut self) -> (XwidgetId, WebViewId) {
        self.xwidget_counter = self.xwidget_counter.wrapping_add(1);
        self.webview_counter = self.webview_counter.wrapping_add(1);
        (
            XwidgetId::new(self.xwidget_counter),
            WebViewId::new(self.webview_counter),
        )
    }

    pub fn ensure_webkit_state(&mut self, id: WebViewId) {
        self.webkit_state.entry(id).or_default();
    }

    pub fn remove_webkit_state(&mut self, id: WebViewId) {
        self.webkit_state.remove(&id);
        self.script_callbacks
            .retain(|_, callback| callback.view != id);
    }

    pub fn next_script_request(&mut self) -> XwidgetScriptRequestId {
        self.script_counter = self.script_counter.wrapping_add(1);
        XwidgetScriptRequestId::new(self.script_counter)
    }

    pub fn register_script_callback(
        &mut self,
        request: XwidgetScriptRequestId,
        view: WebViewId,
        function: Value,
    ) {
        let replaced = self
            .script_callbacks
            .insert(request, PendingScriptCallback { view, function });
        debug_assert!(replaced.is_none(), "script request IDs must be unique");
    }

    pub fn remove_script_callback(&mut self, request: XwidgetScriptRequestId) {
        self.script_callbacks.remove(&request);
    }

    pub fn webkit_uri(&self, id: WebViewId) -> String {
        self.webkit_state
            .get(&id)
            .map(|state| state.uri.clone())
            .unwrap_or_default()
    }

    pub fn set_webkit_uri(&mut self, id: WebViewId, uri: String) {
        self.webkit_state.entry(id).or_default().uri = uri;
    }

    pub fn webkit_load_progress(&self, id: WebViewId) -> f64 {
        self.webkit_state
            .get(&id)
            .map(|state| state.load_progress)
            .unwrap_or(0.0)
    }

    pub fn set_webkit_load_progress(&mut self, id: WebViewId, progress: f64) {
        self.webkit_state.entry(id).or_default().load_progress = progress;
    }

    pub fn webkit_title(&self, id: WebViewId) -> String {
        self.webkit_state
            .get(&id)
            .map(|state| state.title.clone())
            .unwrap_or_default()
    }

    pub fn set_webkit_title(&mut self, id: WebViewId, title: String) {
        self.webkit_state.entry(id).or_default().title = title;
    }

    /// The live xwidget object whose browser instance is `id`, if any.
    pub fn xwidget_for_webview(&self, id: WebViewId) -> Option<Value> {
        let mut rest = self.internal_xwidget_list;
        while rest.is_cons() {
            let candidate = rest.cons_car();
            if candidate
                .as_xwidget()
                .is_some_and(|xwidget| xwidget.webview_id == id)
            {
                return Some(candidate);
            }
            rest = rest.cons_cdr();
        }
        None
    }

    pub fn publish(&self, vars: &mut VariableStore<'_>) {
        vars.store_by_name(
            "xwidget-list",
            shallow_copy_list(self.internal_xwidget_list),
        );
        vars.store_by_name(
            "xwidget-view-list",
            shallow_copy_list(self.internal_xwidget_view_list),
        );
    }
}
fn shallow_copy_list(list: Value) -> Value {
    if list.is_nil() {
        return Value::NIL;
    }
    let mut items = Vec::new();
    let mut tail = list;
    while tail.is_cons() {
        items.push(tail.cons_car());
        tail = tail.cons_cdr();
    }
    assert!(tail.is_nil(), "xwidget internal list must be proper");
    Value::list(items)
}
