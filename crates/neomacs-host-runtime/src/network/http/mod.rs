//! Compile-target-selected HTTP host interface.
mod types;
pub use types::{Error, Request, RequestId, Response};

std::cfg_select! {
    target_family = "wasm" => {
        mod browser;
        pub use browser::{BrowserHttp, start, take, cancel};
    }
    _ => {
        mod native;
        pub use native::{start, take, cancel};
    }
}
