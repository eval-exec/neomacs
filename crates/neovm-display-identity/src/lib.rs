//! Display identity vocabulary shared by the value layer and display protocol.
//!
//! These are the layout-bearing identities embedded in moved object headers
//! (`VideoId`, `WebViewId`, and the resolved font identity vocabulary).
//! `neomacs-display-protocol` re-exports every type at its historical path.
//! This crate must never depend on the display protocol or on a `neovm-*`
//! runtime crate; its only dependency is serde.

/// One id-newtype definition shared by both crates.
///
/// `$crate` paths keep the expansion valid from either crate: serde is reached
/// through the re-export below so an invoking crate needs no serde import of
/// its own, and formatting uses fully qualified core paths.
#[doc(hidden)]
pub use serde as __serde;

#[macro_export]
macro_rules! display_id_type {
    ($name:ident, $raw:ty) => {
        #[repr(transparent)]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Default,
            $crate::__serde::Serialize,
            $crate::__serde::Deserialize,
        )]
        pub struct $name($raw);

        impl $name {
            #[must_use]
            pub const fn new(raw: $raw) -> Self {
                Self(raw)
            }

            #[must_use]
            pub const fn get(self) -> $raw {
                self.0
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

pub mod font;
pub mod video_webview;

pub use font::{
    BackendFontSelector, FontBackendKind, FontVariationCoord, FontVariationSet,
    ResolvedFontIdentity,
};
pub use video_webview::{VideoId, WebViewId};

// The moved headers embed these values by value; pin the sizes and alignments
// the extraction measured at ad42e0ff94. A layout change is a header ABI
// change and must stop the cut.
const _: () = {
    use core::mem::{align_of, size_of};

    assert!(size_of::<video_webview::VideoId>() == 4);
    assert!(align_of::<video_webview::VideoId>() == 4);
    assert!(size_of::<video_webview::WebViewId>() == 4);
    assert!(align_of::<video_webview::WebViewId>() == 4);
    assert!(size_of::<font::FontBackendKind>() == 1);
    assert!(align_of::<font::FontBackendKind>() == 1);
    assert!(size_of::<font::BackendFontSelector>() == 4);
    assert!(align_of::<font::BackendFontSelector>() == 4);
    assert!(size_of::<font::FontVariationCoord>() == 8);
    assert!(align_of::<font::FontVariationCoord>() == 4);
    assert!(size_of::<font::FontVariationSet>() == 24);
    assert!(align_of::<font::FontVariationSet>() == 8);
    assert!(size_of::<font::ResolvedFontIdentity>() == 104);
    assert!(align_of::<font::ResolvedFontIdentity>() == 8);
};
