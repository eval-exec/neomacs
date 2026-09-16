//! Lisp's external-browser operation, distinct from fetching a URL into Emacs.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use crate::emacs_core::error::{EvalResult, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

fn open_external_url(_ctx: &mut Context, url: Value) -> EvalResult {
    let url = url
        .as_str_owned()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("stringp"), url]))?;
    neomacs_host_runtime::navigation::open_external_url(&url)
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    Ok(Value::NIL)
}
