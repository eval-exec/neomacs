use super::super::SubrSpec;

crate::emacs_core::subr::define_subrs! {
    SubrSpec::fixed0("test-batch-zero", super::zero),
}
