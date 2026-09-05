//! Existing native Lisp/socket networking is deliberately unchanged.
use super::{Error, Request, RequestId, Response};

/// Browser-only request creation is unavailable on native targets.
pub fn start(_: Request) -> Result<RequestId, Error> {
    Err(Error::Unavailable)
}
/// Browser-only result retrieval is unavailable on native targets.
pub fn take(_: RequestId) -> Result<Option<Response>, Error> {
    Err(Error::Unavailable)
}
/// Native targets have no browser requests to cancel.
pub fn cancel(_: RequestId) {}
