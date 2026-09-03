//! Native Lisp declarations for the Windows platform surface.

use super::*;
use crate::emacs_core::subr::{FixedMin1, NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    native_host;
    SubrSpec::fixed1("w32-short-file-name", w32_short_file_name, FixedMin1::One),
    SubrSpec::fixed1("w32-long-file-name", w32_long_file_name, FixedMin1::One),
    SubrSpec::fixed0("w32-get-valid-codepages", w32_get_valid_codepages),
    SubrSpec::fixed0("w32-get-console-codepage", w32_get_console_codepage),
    SubrSpec::fixed1(
        "w32-set-console-codepage",
        w32_set_console_codepage,
        FixedMin1::One,
    ),
    SubrSpec::fixed0(
        "w32-get-console-output-codepage",
        w32_get_console_output_codepage,
    ),
    SubrSpec::fixed1(
        "w32-set-console-output-codepage",
        w32_set_console_output_codepage,
        FixedMin1::One,
    ),
    SubrSpec::fixed1(
        "w32-get-codepage-charset",
        w32_get_codepage_charset,
        FixedMin1::One,
    ),
}
