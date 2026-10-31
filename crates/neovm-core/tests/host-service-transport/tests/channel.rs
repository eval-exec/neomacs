//! Real production transport, with metadata/native DTO shims only.
#![forbid(unsafe_code)]
#![allow(dead_code)]

mod host_effect {
    pub mod contracts {
        #[derive(Debug, PartialEq, Eq)]
        pub enum OwnedNativeData {
            None,
            Text(String),
            Bytes(Vec<u8>),
            Truth(NativeTruth),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum NativeTruth {
            False,
            True,
        }
    }
}

mod emacs_core;
