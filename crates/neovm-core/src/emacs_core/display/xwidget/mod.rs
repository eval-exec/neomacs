//! GNU-shaped xwidget model and view runtime.
//!
//! GNU Emacs stores xwidgets as `PVEC_XWIDGET` pseudovectors and xwidget views
//! as `PVEC_XWIDGET_VIEW` pseudovectors.  This module owns the evaluator-side
//! lists and builtins for that object model; native frontend/embedder state is
//! intentionally kept out of the Lisp heap objects.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use super::builtins::{
    builtin_get_buffer, builtin_get_buffer_create, collect_proper_list_items, expect_wholenump,
};
use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::symbol::Obarray;
use super::value::{Value, eq_value};
use crate::emacs_core::display_host::XwidgetScriptRequestId;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args_range;
use crate::heap_types::LispString;
use crate::keyboard::{FrontendWebValue, FrontendWebViewEvent};
use neomacs_display_protocol::{WebViewId, XwidgetId};
use std::collections::HashMap;
use strum::IntoStaticStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum XwidgetType {
    Webkit,
}

impl XwidgetType {
    fn is_lisp_value(self, value: Value) -> bool {
        value == self.value()
    }

    fn value(self) -> Value {
        Value::symbol(self.name())
    }

    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Debug, Default)]
struct WebKitRuntimeState {
    generation: u64,
    uri: String,
    title: String,
    /// Estimated load progress, 0.0..=1.0, as `xwidget-webkit-estimated-load-
    /// progress' reports it.
    ///
    /// GNU reads WebKitGTK's continuous `estimated-load-progress' property.
    /// Neomacs initializes this optimistically when dispatching navigation,
    /// then replaces it with generation-qualified measurements from backends
    /// that expose progress (WPEPlatform and WKWebView).
    load_progress: f64,
}

#[derive(Clone, Debug)]
struct PendingScriptCallback {
    view: WebViewId,
    function: Value,
}

pub(crate) enum XwidgetFrontendEffect {
    None,
    Redisplay,
    InvokeScriptCallback { function: Value, argument: Value },
}

impl XwidgetFrontendEffect {
    pub(crate) const fn redisplay_needed(&self) -> bool {
        matches!(self, Self::Redisplay | Self::InvokeScriptCallback { .. })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct XwidgetState {
    internal_xwidget_list: Value,
    internal_xwidget_view_list: Value,
    webkit_state: HashMap<WebViewId, WebKitRuntimeState>,
    script_callbacks: HashMap<XwidgetScriptRequestId, PendingScriptCallback>,
    xwidget_counter: u32,
    webview_counter: u32,
    script_counter: u64,
}

impl XwidgetState {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        visit(self.internal_xwidget_list);
        visit(self.internal_xwidget_view_list);
        for callback in self.script_callbacks.values() {
            visit(callback.function);
        }
    }

    fn next_ids(&mut self) -> (XwidgetId, WebViewId) {
        self.xwidget_counter = self.xwidget_counter.wrapping_add(1);
        self.webview_counter = self.webview_counter.wrapping_add(1);
        (
            XwidgetId::new(self.xwidget_counter),
            WebViewId::new(self.webview_counter),
        )
    }

    fn ensure_webkit_state(&mut self, id: WebViewId) {
        self.webkit_state.entry(id).or_default();
    }

    fn remove_webkit_state(&mut self, id: WebViewId) {
        self.webkit_state.remove(&id);
        self.script_callbacks
            .retain(|_, callback| callback.view != id);
    }

    fn next_script_request(&mut self) -> XwidgetScriptRequestId {
        self.script_counter = self.script_counter.wrapping_add(1);
        XwidgetScriptRequestId::new(self.script_counter)
    }

    fn register_script_callback(
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

    fn remove_script_callback(&mut self, request: XwidgetScriptRequestId) {
        self.script_callbacks.remove(&request);
    }

    fn webkit_uri(&self, id: WebViewId) -> String {
        self.webkit_state
            .get(&id)
            .map(|state| state.uri.clone())
            .unwrap_or_default()
    }

    fn set_webkit_uri(&mut self, id: WebViewId, uri: String) {
        self.webkit_state.entry(id).or_default().uri = uri;
    }

    fn webkit_load_progress(&self, id: WebViewId) -> f64 {
        self.webkit_state
            .get(&id)
            .map(|state| state.load_progress)
            .unwrap_or(0.0)
    }

    fn set_webkit_load_progress(&mut self, id: WebViewId, progress: f64) {
        self.webkit_state.entry(id).or_default().load_progress = progress;
    }

    fn webkit_title(&self, id: WebViewId) -> String {
        self.webkit_state
            .get(&id)
            .map(|state| state.title.clone())
            .unwrap_or_default()
    }

    fn set_webkit_title(&mut self, id: WebViewId, title: String) {
        self.webkit_state.entry(id).or_default().title = title;
    }

    pub(crate) fn apply_frontend_event(
        &mut self,
        event: &FrontendWebViewEvent,
    ) -> XwidgetFrontendEffect {
        match event {
            FrontendWebViewEvent::Ready { id, generation } => {
                let state = self.webkit_state.entry(*id).or_default();
                if *generation >= state.generation {
                    state.generation = *generation;
                }
                XwidgetFrontendEffect::None
            }
            FrontendWebViewEvent::TitleChanged {
                id,
                generation,
                title,
            } if self
                .webkit_state
                .get(id)
                .is_some_and(|state| state.generation == *generation) =>
            {
                self.set_webkit_title(*id, title.clone());
                XwidgetFrontendEffect::Redisplay
            }
            FrontendWebViewEvent::UriChanged {
                id,
                generation,
                uri,
            } if self
                .webkit_state
                .get(id)
                .is_some_and(|state| state.generation == *generation) =>
            {
                self.set_webkit_uri(*id, uri.clone());
                XwidgetFrontendEffect::Redisplay
            }
            FrontendWebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress,
            } if self
                .webkit_state
                .get(id)
                .is_some_and(|state| state.generation == *generation) =>
            {
                self.set_webkit_load_progress(*id, progress.clamp(0.0, 1.0));
                XwidgetFrontendEffect::Redisplay
            }
            FrontendWebViewEvent::LoadFinished { id, generation, .. }
                if self
                    .webkit_state
                    .get(id)
                    .is_some_and(|state| state.generation == *generation) =>
            {
                self.set_webkit_load_progress(*id, 1.0);
                XwidgetFrontendEffect::Redisplay
            }
            FrontendWebViewEvent::ScriptFinished {
                view,
                generation,
                request,
                result,
            } if self
                .webkit_state
                .get(view)
                .is_some_and(|state| state.generation == *generation) =>
            {
                let request = XwidgetScriptRequestId::new(*request);
                let Some(callback) = self.script_callbacks.get(&request) else {
                    return XwidgetFrontendEffect::None;
                };
                if callback.view != *view {
                    return XwidgetFrontendEffect::None;
                }
                let callback = self
                    .script_callbacks
                    .remove(&request)
                    .expect("the pending script callback was checked");
                match result {
                    Ok(value) => XwidgetFrontendEffect::InvokeScriptCallback {
                        function: callback.function,
                        argument: frontend_web_value_to_lisp(value),
                    },
                    // GNU frees the saved callback and does not invoke FUN
                    // when WebKit reports a JavaScript error.
                    Err(_) => XwidgetFrontendEffect::None,
                }
            }
            FrontendWebViewEvent::Failed { id, generation, .. }
            | FrontendWebViewEvent::Closed { id, generation }
            | FrontendWebViewEvent::ProcessFailed { id, generation, .. }
                if self
                    .webkit_state
                    .get(id)
                    .is_some_and(|state| state.generation == *generation) =>
            {
                self.script_callbacks
                    .retain(|_, callback| callback.view != *id);
                XwidgetFrontendEffect::None
            }
            FrontendWebViewEvent::Failed { .. }
            | FrontendWebViewEvent::Closed { .. }
            | FrontendWebViewEvent::ProcessFailed { .. }
            | FrontendWebViewEvent::TitleChanged { .. }
            | FrontendWebViewEvent::UriChanged { .. }
            | FrontendWebViewEvent::LoadProgressChanged { .. }
            | FrontendWebViewEvent::LoadFinished { .. }
            | FrontendWebViewEvent::ScriptFinished { .. }
            | FrontendWebViewEvent::FocusChanged { .. } => XwidgetFrontendEffect::None,
        }
    }

    fn publish(&self, obarray: &mut Obarray) {
        obarray.set_symbol_value(
            "xwidget-list",
            shallow_copy_list(self.internal_xwidget_list),
        );
        obarray.set_symbol_value(
            "xwidget-view-list",
            shallow_copy_list(self.internal_xwidget_view_list),
        );
    }
}

fn frontend_web_value_to_lisp(value: &FrontendWebValue) -> Value {
    match value {
        FrontendWebValue::Null => Value::NIL,
        FrontendWebValue::Bool(value) => Value::bool_val(*value),
        FrontendWebValue::Number(value) => Value::make_float(*value),
        FrontendWebValue::String(value) => Value::string(value),
        FrontendWebValue::Array(values) => {
            Value::vector(values.iter().map(frontend_web_value_to_lisp).collect())
        }
        // GNU's JavaScriptCore conversion represents an object as a vector of
        // (string . value) pairs. BTreeMap preserves deterministic key order.
        FrontendWebValue::Object(values) => Value::vector(
            values
                .iter()
                .map(|(key, value)| {
                    Value::cons(Value::string(key), frontend_web_value_to_lisp(value))
                })
                .collect(),
        ),
    }
}

impl Context {
    pub(crate) fn apply_xwidget_frontend_event(
        &mut self,
        event: &FrontendWebViewEvent,
    ) -> Result<bool, Flow> {
        let effect = self.xwidgets.apply_frontend_event(event);
        let redisplay = effect.redisplay_needed();
        if let XwidgetFrontendEffect::InvokeScriptCallback { function, argument } = effect {
            self.funcall_general(function, vec![argument])?;
        }
        Ok(redisplay)
    }
}

pub(crate) fn init_xwidget_variables(obarray: &mut Obarray) {
    obarray.set_symbol_value("xwidget-list", Value::NIL);
    obarray.make_special("xwidget-list");
    obarray.set_symbol_value("xwidget-view-list", Value::NIL);
    obarray.make_special("xwidget-view-list");
    obarray.set_symbol_value("xwidget-webkit-disable-javascript", Value::NIL);
    obarray.make_special("xwidget-webkit-disable-javascript");
}

fn shallow_copy_list(list: Value) -> Value {
    if list.is_nil() {
        return Value::NIL;
    }
    let items = collect_proper_list_items(list).expect("xwidget internal list must be proper");
    Value::list(items)
}

fn delq_from_list(list: Value, target: Value) -> Value {
    let items = collect_proper_list_items(list).expect("xwidget internal list must be proper");
    Value::list(
        items
            .into_iter()
            .filter(|item| !eq_value(item, &target))
            .collect(),
    )
}

fn expect_xwidget(value: Value) -> Result<Value, Flow> {
    if value.is_xwidget() {
        Ok(value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("xwidgetp"), value],
        ))
    }
}

fn expect_live_xwidget(value: Value) -> Result<Value, Flow> {
    if value
        .as_xwidget()
        .is_some_and(|xwidget| !xwidget.buffer.is_nil())
    {
        Ok(value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("xwidget-live-p"), value],
        ))
    }
}

fn expect_live_webkit_xwidget(value: Value) -> Result<Value, Flow> {
    let value = expect_live_xwidget(value)?;
    let xwidget = value.as_xwidget().unwrap();
    if XwidgetType::Webkit.is_lisp_value(xwidget.type_) {
        Ok(value)
    } else {
        Err(signal("error", vec![Value::string("Not a WebKit widget")]))
    }
}

fn expect_xwidget_view(value: Value) -> Result<Value, Flow> {
    if value.is_xwidget_view() {
        Ok(value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("xwidget-view-p"), value],
        ))
    }
}

fn expect_buffer(value: Value) -> Result<Value, Flow> {
    if value.is_buffer() {
        Ok(value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), value],
        ))
    }
}

fn expect_symbol(value: Value) -> Result<Value, Flow> {
    if value.is_symbol() {
        Ok(value)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), value],
        ))
    }
}

fn expect_string(value: Value) -> Result<LispString, Flow> {
    value.as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })
}

fn expect_i32_wholenump(value: Value) -> Result<i32, Flow> {
    let n = expect_wholenump(&value)?;
    i32::try_from(n).map_err(|_| {
        signal(
            LispCondition::ArgsOutOfRange,
            vec![value, Value::fixnum(0), Value::fixnum(i32::MAX as i64)],
        )
    })
}

fn ensure_proper_list(value: Value) -> Result<(), Flow> {
    collect_proper_list_items(value).map(|_| ())
}

fn current_buffer_value(eval: &Context) -> EvalResult {
    let id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::make_buffer(id))
}

fn xwidget_live_p_value(value: Value) -> bool {
    value
        .as_xwidget()
        .is_some_and(|xwidget| !xwidget.buffer.is_nil())
}

fn create(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("make-xwidget", &args, 4, 7)?;
    let type_ = expect_symbol(args[0])?;
    if !XwidgetType::Webkit.is_lisp_value(type_) {
        return Err(signal("error", vec![Value::string("Bad xwidget type")]));
    }
    eval.require_value(Value::symbol("xwidget"), None, None)?;
    let title = args[1];
    let width = expect_i32_wholenump(args[2])?;
    let height = expect_i32_wholenump(args[3])?;
    let buffer_arg = args.get(5).copied().unwrap_or(Value::NIL);
    let buffer = if buffer_arg.is_nil() {
        current_buffer_value(eval)?
    } else {
        builtin_get_buffer_create(eval, vec![buffer_arg, Value::NIL])?
    };
    let (xwidget_id, webview_id) = eval.xwidgets.next_ids();
    let xwidget = Value::make_xwidget(
        type_,
        title,
        buffer,
        width,
        height,
        xwidget_id.get(),
        webview_id,
    );
    if let Some(host) = eval.display_host.as_ref() {
        host.create_webkit_xwidget(webview_id, width.max(1) as u32, height.max(1) as u32)
            .map_err(|err| signal("error", vec![Value::string(err)]))?;
    }
    eval.xwidgets.ensure_webkit_state(webview_id);
    eval.xwidgets.internal_xwidget_list = Value::cons(xwidget, eval.xwidgets.internal_xwidget_list);
    eval.xwidgets.publish(&mut eval.obarray);
    Ok(xwidget)
}

fn is_xwidget(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidgetp", &args, 1, 1)?;
    Ok(Value::bool_val(args[0].is_xwidget()))
}

fn is_view(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-view-p", &args, 1, 1)?;
    Ok(Value::bool_val(args[0].is_xwidget_view()))
}

fn is_live(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-live-p", &args, 1, 1)?;
    Ok(Value::bool_val(xwidget_live_p_value(args[0])))
}

fn info(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-info", &args, 1, 1)?;
    let value = expect_live_xwidget(args[0])?;
    let xwidget = value.as_xwidget().unwrap();
    Ok(Value::vector(vec![
        xwidget.type_,
        xwidget.title,
        Value::fixnum(xwidget.width as i64),
        Value::fixnum(xwidget.height as i64),
    ]))
}

fn view_info(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-view-info", &args, 1, 1)?;
    let value = expect_xwidget_view(args[0])?;
    let view = value.as_xwidget_view().unwrap();
    Ok(Value::vector(vec![
        Value::fixnum(view.x as i64),
        Value::fixnum(view.y as i64),
        Value::fixnum(view.clip_right as i64),
        Value::fixnum(view.clip_bottom as i64),
        Value::fixnum(view.clip_top as i64),
        Value::fixnum(view.clip_left as i64),
    ]))
}

fn view_model(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-view-model", &args, 1, 1)?;
    let value = expect_xwidget_view(args[0])?;
    Ok(value.as_xwidget_view().unwrap().model)
}

fn view_window(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-view-window", &args, 1, 1)?;
    let value = expect_xwidget_view(args[0])?;
    Ok(value.as_xwidget_view().unwrap().window)
}

fn lookup_view(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-view-lookup", &args, 1, 2)?;
    let model = expect_live_xwidget(args[0])?;
    let window = match args.get(1).copied().filter(|window| !window.is_nil()) {
        None => Value::make_window(super::window_cmds::selected_window_id(eval)?.0),
        Some(window) if window.is_window() => window,
        Some(window) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("windowp"), window],
            ));
        }
    };
    let items = collect_proper_list_items(eval.xwidgets.internal_xwidget_view_list)?;
    for view_value in items {
        let Some(view) = view_value.as_xwidget_view() else {
            continue;
        };
        if eq_value(&view.model, &model) && eq_value(&view.window, &window) {
            return Ok(view_value);
        }
    }
    Ok(Value::NIL)
}

fn delete_view(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("delete-xwidget-view", &args, 1, 1)?;
    let value = expect_xwidget_view(args[0])?;
    eval.xwidgets.internal_xwidget_view_list =
        delq_from_list(eval.xwidgets.internal_xwidget_view_list, value);
    eval.xwidgets.publish(&mut eval.obarray);
    Ok(Value::NIL)
}

fn plist(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-plist", &args, 1, 1)?;
    let value = expect_live_xwidget(args[0])?;
    Ok(value.as_xwidget().unwrap().plist)
}

fn set_plist(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("set-xwidget-plist", &args, 2, 2)?;
    let value = expect_live_xwidget(args[0])?;
    let plist = args[1];
    ensure_proper_list(plist)?;
    value.with_xwidget_mut(|xwidget| {
        xwidget.plist = plist;
    });
    Ok(plist)
}

fn buffer(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-buffer", &args, 1, 1)?;
    let value = expect_xwidget(args[0])?;
    Ok(value.as_xwidget().unwrap().buffer)
}

fn set_buffer(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("set-xwidget-buffer", &args, 2, 2)?;
    let value = expect_live_xwidget(args[0])?;
    let buffer = expect_buffer(args[1])?;
    value.with_xwidget_mut(|xwidget| {
        xwidget.buffer = buffer;
    });
    Ok(Value::NIL)
}

fn query_on_exit(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-query-on-exit-flag", &args, 1, 1)?;
    let value = expect_live_xwidget(args[0])?;
    Ok(Value::bool_val(
        !value.as_xwidget().unwrap().kill_without_query,
    ))
}

fn set_query_on_exit(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("set-xwidget-query-on-exit-flag", &args, 2, 2)?;
    let value = expect_live_xwidget(args[0])?;
    let flag = args[1];
    value.with_xwidget_mut(|xwidget| {
        xwidget.kill_without_query = flag.is_nil();
    });
    Ok(flag)
}

fn buffer_xwidgets(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("get-buffer-xwidgets", &args, 1, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    let buffer = builtin_get_buffer(eval, vec![args[0]])?;
    if buffer.is_nil() {
        return Ok(Value::NIL);
    }
    let items = collect_proper_list_items(eval.xwidgets.internal_xwidget_list)?;
    let mut result = Value::NIL;
    for value in items {
        let Some(xwidget) = value.as_xwidget() else {
            continue;
        };
        if eq_value(&xwidget.buffer, &buffer) {
            result = Value::cons(value, result);
        }
    }
    Ok(result)
}

fn kill(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("kill-xwidget", &args, 1, 1)?;
    let value = expect_live_xwidget(args[0])?;
    let id = value.as_xwidget().unwrap().webview_id;
    eval.xwidgets.internal_xwidget_list =
        delq_from_list(eval.xwidgets.internal_xwidget_list, value);
    eval.xwidgets.publish(&mut eval.obarray);
    if let Some(host) = eval.display_host.as_ref() {
        host.destroy_webkit_xwidget(id)
            .map_err(|err| signal("error", vec![Value::string(err)]))?;
    }
    eval.xwidgets.remove_webkit_state(id);
    value.with_xwidget_mut(|xwidget| {
        xwidget.buffer = Value::NIL;
    });
    Ok(Value::NIL)
}

fn resize(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-resize", &args, 3, 3)?;
    let value = expect_live_xwidget(args[0])?;
    let width = expect_i32_wholenump(args[1])?;
    let height = expect_i32_wholenump(args[2])?;
    let id = value.as_xwidget().unwrap().webview_id;
    value.with_xwidget_mut(|xwidget| {
        xwidget.width = width;
        xwidget.height = height;
    });
    if let Some(host) = eval.display_host.as_ref() {
        host.resize_webkit_xwidget(id, width.max(1) as u32, height.max(1) as u32)
            .map_err(|err| signal("error", vec![Value::string(err)]))?;
    }
    Ok(Value::NIL)
}

fn webkit_uri(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-webkit-uri", &args, 1, 1)?;
    let value = expect_live_webkit_xwidget(args[0])?;
    let id = value.as_xwidget().unwrap().webview_id;
    Ok(Value::string(_eval.xwidgets.webkit_uri(id)))
}

fn webkit_title(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-webkit-title", &args, 1, 1)?;
    let value = expect_live_webkit_xwidget(args[0])?;
    let id = value.as_xwidget().unwrap().webview_id;
    Ok(Value::string(_eval.xwidgets.webkit_title(id)))
}

fn navigate_webkit(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-webkit-goto-uri", &args, 2, 2)?;
    let value = expect_live_webkit_xwidget(args[0])?;
    let uri = expect_string(args[1])?;
    let id = value.as_xwidget().unwrap().webview_id;
    if let Some(host) = eval.display_host.as_ref() {
        host.load_webkit_xwidget_uri(id, uri.clone())
            .map_err(|err| signal("error", vec![Value::string(err)]))?;
    }
    eval.xwidgets
        .set_webkit_uri(id, String::from_utf8_lossy(uri.as_bytes()).into_owned());
    // Establish an immediate optimistic value; a measured backend event can
    // replace it after the frontend acknowledges the active generation.
    eval.xwidgets.set_webkit_load_progress(id, 1.0);
    Ok(Value::NIL)
}

fn execute_script(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-webkit-execute-script", &args, 2, 3)?;
    let value = expect_live_webkit_xwidget(args[0])?;
    let script = expect_string(args[1])?;
    let id = value.as_xwidget().unwrap().webview_id;
    let callback = args.get(2).copied().filter(|fun| !fun.is_nil());
    if callback.is_some_and(|fun| !eval.function_value_is_callable(&fun)) {
        return Err(signal(
            LispCondition::InvalidFunction,
            vec![callback.expect("the callback was checked")],
        ));
    }
    let request = eval.xwidgets.next_script_request();
    if let Some(function) = callback {
        eval.xwidgets
            .register_script_callback(request, id, function);
    }
    if let Some(host) = eval.display_host.as_ref() {
        if let Err(error) = host.execute_webkit_xwidget_script(id, request, script) {
            eval.xwidgets.remove_script_callback(request);
            return Err(signal("error", vec![Value::string(error)]));
        }
    } else {
        eval.xwidgets.remove_script_callback(request);
    }
    Ok(Value::NIL)
}

fn estimated_load_progress(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-webkit-estimated-load-progress", &args, 1, 1)?;
    let value = expect_live_webkit_xwidget(args[0])?;
    let id = value.as_xwidget().unwrap().webview_id;
    Ok(Value::make_float(_eval.xwidgets.webkit_load_progress(id)))
}

fn size_request(_eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("xwidget-size-request", &args, 1, 1)?;
    let value = expect_live_xwidget(args[0])?;
    let xwidget = value.as_xwidget().unwrap();
    Ok(Value::list(vec![
        Value::fixnum(xwidget.width as i64),
        Value::fixnum(xwidget.height as i64),
    ]))
}
