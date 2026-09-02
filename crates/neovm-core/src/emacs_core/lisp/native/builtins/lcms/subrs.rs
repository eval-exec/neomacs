//! Native Lisp declarations for Little CMS support.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    target_filtered;
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-cie-de2000",
        NativeFn::NoContextVec(lcms_cie_de2000),
        SubrArity::new(2, Some(5)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-xyz->jch",
        NativeFn::NoContextVec(lcms_xyz_to_jch),
        SubrArity::new(1, Some(3)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-jch->xyz",
        NativeFn::NoContextVec(lcms_jch_to_xyz),
        SubrArity::new(1, Some(3)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-jch->jab",
        NativeFn::NoContextVec(lcms_jch_to_jab),
        SubrArity::new(1, Some(3)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-jab->jch",
        NativeFn::NoContextVec(lcms_jab_to_jch),
        SubrArity::new(1, Some(3)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-cam02-ucs",
        NativeFn::NoContextVec(lcms_cam02_ucs),
        SubrArity::new(2, Some(4)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms2-available-p",
        NativeFn::NoContextVec(lcms2_available_p),
        SubrArity::new(0, Some(0)),
    ),
    #[cfg(all(neomacs_have_lcms2, not(target_family = "wasm")))]
    SubrSpec::new(
        "lcms-temp->white-point",
        NativeFn::NoContextVec(lcms_temp_to_white_point),
        SubrArity::new(1, Some(1)),
    ),
}
