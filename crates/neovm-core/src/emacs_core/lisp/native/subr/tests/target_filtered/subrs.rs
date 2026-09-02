crate::emacs_core::subr::define_subrs! {
    target_filtered;
    #[cfg(any())]
    crate::emacs_core::subr::SubrSpec::fixed0("never-compiled", super::super::zero),
}
