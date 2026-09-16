//! The external-browser capability has the same Lisp arity on every target.
use super::*;
use crate::emacs_core::subr::{FixedMin1, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::fixed1("neomacs-open-external-url", open_external_url, FixedMin1::One),
}
