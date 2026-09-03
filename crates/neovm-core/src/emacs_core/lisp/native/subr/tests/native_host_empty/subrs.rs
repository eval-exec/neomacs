crate::emacs_core::subr::define_subrs! {
    native_host;
    #[cfg(any())]
    crate::emacs_core::subr::SubrSpec::fixed0("never-compiled", super::super::zero),
}
