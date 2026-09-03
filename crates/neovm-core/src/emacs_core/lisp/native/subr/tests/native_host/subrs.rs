use super::super::SubrSpec;

crate::emacs_core::subr::define_subrs! {
    native_host;
    SubrSpec::fixed0("test-native-host-zero", super::super::zero),
}
