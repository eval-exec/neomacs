//! Native Lisp declarations for the Windows platform surface.

use super::*;
use crate::emacs_core::subr::{FixedMin1, NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1("w32-short-file-name", w32_short_file_name, FixedMin1::One)),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1("w32-long-file-name", w32_long_file_name, FixedMin1::One)),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed0("w32-get-valid-codepages", w32_get_valid_codepages)),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed0("w32-get-console-codepage", w32_get_console_codepage)),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "w32-set-console-codepage",
        w32_set_console_codepage,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "w32-get-console-output-codepage",
        w32_get_console_output_codepage,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "w32-set-console-output-codepage",
        w32_set_console_output_codepage,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "w32-get-codepage-charset",
        w32_get_codepage_charset,
        FixedMin1::One,
    )),
}
