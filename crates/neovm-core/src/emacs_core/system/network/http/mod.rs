//! Lisp HTTP host operations. Lisp owns callbacks and request-buffer lifetime.
mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use crate::emacs_core::error::{EvalResult, Flow, expect_args, signal};
use crate::emacs_core::value::{Value, list_to_vec};
use crate::heap_types::LispString;
use neomacs_host_runtime::network::http::{self, Request, RequestId};

fn error(message: impl ToString) -> Flow {
    signal("error", vec![Value::string(message.to_string())])
}

fn text(value: &Value) -> Result<String, Flow> {
    value.as_str_owned().ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), *value],
        )
    })
}

fn request_id(value: &Value) -> Result<RequestId, Flow> {
    value
        .as_fixnum()
        .and_then(|n| u32::try_from(n).ok())
        .and_then(RequestId::new)
        .ok_or_else(|| error("invalid HTTP request identifier"))
}

fn start(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-http-start", &args, 4)?;
    let mut headers = Vec::new();
    for pair in list_to_vec(&args[2]).ok_or_else(|| error("HTTP headers must be an alist"))? {
        if !pair.is_cons() {
            return Err(error("HTTP header must be a pair"));
        }
        let (name, value) = (pair.cons_car(), pair.cons_cdr());
        headers.push((text(&name)?, text(&value)?));
    }
    let body = if args[3].is_nil() {
        None
    } else {
        let value = args[3]
            .as_lisp_string()
            .ok_or_else(|| error("HTTP body must be a string"))?;
        if value.is_multibyte() {
            return Err(error("encode the HTTP body as unibyte data first"));
        }
        Some(value.as_bytes().to_vec())
    };
    let id = http::start(Request {
        url: text(&args[0])?,
        method: text(&args[1])?,
        headers,
        body,
    })
    .map_err(error)?;
    Ok(Value::fixnum(i64::from(id.get())))
}

fn take(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-http-take", &args, 1)?;
    let Some(response) = http::take(request_id(&args[0])?).map_err(error)? else {
        return Ok(Value::NIL);
    };
    let headers = Value::list(
        response
            .headers
            .into_iter()
            .map(|(name, value)| Value::cons(Value::string(name), Value::string(value)))
            .collect(),
    );
    Ok(Value::vector(vec![
        Value::fixnum(i64::from(response.status)),
        Value::string(response.url),
        headers,
        Value::heap_string(LispString::from_unibyte(response.body)),
    ]))
}

fn cancel(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-http-cancel", &args, 1)?;
    http::cancel(request_id(&args[0])?);
    Ok(Value::NIL)
}
