//! One schema for the typed specpdl and its JIT-visible C-layout records.
//!
//! A primitive-representation enum is a union of C-layout variant records,
//! each beginning with its discriminant. Generate both views from the same
//! list so a payload change cannot silently leave the JIT's offsets behind.
//! These records describe layout only; they never carry a second copy of
//! the owning Context's thread-confined bindings.

macro_rules! define {
    (
        $(#[$enum_attr:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$attr:meta])*
                $variant:ident $( { $($field:ident: $ty:ty),* $(,)? } )?
            ),* $(,)?
        }
    ) => {
        $(#[$enum_attr])*
        #[repr(u8)]
        #[derive(Clone, Debug, strum::EnumDiscriminants)]
        #[strum_discriminants(name(SpecBindingTag))]
        $vis enum $name {
            $( $(#[$attr])* $variant $( { $($field: $ty),* } )? ),*
        }

        #[cfg(feature = "jit")]
        // Most variants need no JIT template. Keeping their records here
        // still makes the complete representation follow the same schema.
        #[allow(dead_code)]
        pub(crate) mod specbinding_records {
            use super::*;
            $(
                #[repr(C)]
                #[derive(Debug)]
                pub(crate) struct $variant {
                    pub(crate) tag: SpecBindingTag,
                    $( $(pub(crate) $field: $ty,)* )?
                }
            )*
        }
    };
}

pub(super) use define;
