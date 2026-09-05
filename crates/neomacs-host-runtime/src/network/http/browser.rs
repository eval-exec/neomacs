//! Browser composition-root callbacks; no runtime platform selection.
use super::{Error, Request, RequestId, Response};
use std::cell::Cell;

/// Raw-worker composition adapter installed once on the VM thread.
#[derive(Clone, Copy)]
pub struct BrowserHttp {
    start: fn(Request) -> Result<RequestId, Error>,
    take: fn(RequestId) -> Result<Option<Response>, Error>,
    cancel: fn(RequestId),
}
thread_local! { static HTTP: Cell<Option<BrowserHttp>> = const { Cell::new(None) }; }

impl BrowserHttp {
    /// Bundle host callbacks without capturing VM state.
    pub fn new(
        start: fn(Request) -> Result<RequestId, Error>,
        take: fn(RequestId) -> Result<Option<Response>, Error>,
        cancel: fn(RequestId),
    ) -> Self {
        Self {
            start,
            take,
            cancel,
        }
    }
    /// Install for the current thread; replacing an active adapter is forbidden.
    pub fn install(self) -> Result<(), Error> {
        HTTP.with(|slot| {
            if slot.get().is_some() {
                return Err(Error::Failed("browser HTTP is already installed".into()));
            }
            slot.set(Some(self));
            Ok(())
        })
    }
}
/// Begin an asynchronous request.
pub fn start(request: Request) -> Result<RequestId, Error> {
    (HTTP.with(Cell::get).ok_or(Error::Unavailable)?.start)(request)
}
/// Consume a completed response; pending requests return None.
pub fn take(id: RequestId) -> Result<Option<Response>, Error> {
    (HTTP.with(Cell::get).ok_or(Error::Unavailable)?.take)(id)
}
/// Cancel and release a request, including any retained completion.
pub fn cancel(id: RequestId) {
    if let Some(http) = HTTP.with(Cell::get) {
        (http.cancel)(id);
    }
}
