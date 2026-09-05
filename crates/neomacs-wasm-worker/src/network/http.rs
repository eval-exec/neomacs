//! Checked raw-worker ABI: metadata is JSON; bodies cross as binary bytes.
use neomacs_host_runtime::network::http::{Error, Request, RequestId, Response};

#[link(wasm_import_module = "neomacs_host")]
unsafe extern "C" {
    safe fn http_start(metadata: *const u8, length: u32, body: *const u8, body_length: u32) -> u32;
    safe fn http_poll(id: u32) -> u32;
    safe fn http_result_len(id: u32, field: u32) -> u32;
    safe fn http_copy_result(id: u32, field: u32, destination: *mut u8, capacity: u32) -> u32;
    safe fn http_cancel(id: u32);
}

pub(crate) fn install() -> Result<(), Error> {
    neomacs_host_runtime::network::http::BrowserHttp::new(start, take, cancel).install()
}

fn start(request: Request) -> Result<RequestId, Error> {
    let metadata = serde_json::to_vec(&serde_json::json!({
        "url": request.url, "method": request.method, "headers": request.headers,
        "hasBody": request.body.is_some(),
    }))
    .map_err(|e| Error::Failed(e.to_string()))?;
    let body = request.body.as_deref().unwrap_or_default();
    if metadata.len() > 64 * 1024 || body.len() > 1024 * 1024 {
        return Err(Error::Failed("HTTP request exceeds host limits".into()));
    }
    RequestId::new(http_start(
        metadata.as_ptr(),
        metadata.len() as u32,
        body.as_ptr(),
        body.len() as u32,
    ))
    .ok_or_else(|| {
        Error::Failed("HTTP request rejected (invalid request or too many pending requests)".into())
    })
}

fn copy(id: RequestId, field: u32, limit: u32) -> Result<Vec<u8>, Error> {
    let length = http_result_len(id.get(), field);
    if length > limit {
        return Err(Error::Failed("HTTP host result exceeds limit".into()));
    }
    let mut bytes = vec![0; length as usize];
    if http_copy_result(id.get(), field, bytes.as_mut_ptr(), length) != length {
        return Err(Error::Failed("HTTP host returned truncated data".into()));
    }
    Ok(bytes)
}

fn take(id: RequestId) -> Result<Option<Response>, Error> {
    let state = http_poll(id.get());
    if state == 0 {
        return Ok(None);
    }
    let result = (|| match state {
        1 => {
            let metadata = copy(id, 0, 64 * 1024)?;
            let (status, url, headers): (u16, String, Vec<(String, String)>) =
                serde_json::from_slice(&metadata).map_err(|e| Error::Failed(e.to_string()))?;
            if !(100..=599).contains(&status) {
                return Err(Error::Failed("HTTP host returned an invalid status".into()));
            }
            Ok(Some(Response {
                status,
                url,
                headers,
                body: copy(id, 1, 16 * 1024 * 1024)?,
            }))
        }
        2 => Err(Error::Failed(
            String::from_utf8_lossy(&copy(id, 0, 64 * 1024)?).into_owned(),
        )),
        _ => Err(Error::UnknownRequest),
    })();
    cancel(id);
    result
}

fn cancel(id: RequestId) {
    http_cancel(id.get());
}
