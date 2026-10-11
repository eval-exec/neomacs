use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;

#[test]
fn gc_tls_ownership_static_subrs_outlive_contexts_without_gc_roots() {
    let original = {
        let mut first = Context::new();
        let value = TaggedValue::subr_from_sym_id(intern("car"));
        first.gc_collect_exact();
        assert_eq!(value.veclike_type(), Some(VecLikeType::Subr));
        value
    };
    let mut next = Context::new();
    let value = TaggedValue::subr_from_sym_id(intern("car"));
    assert_eq!(value.bits(), original.bits());
    next.gc_collect_exact();
    assert_eq!(original.veclike_type(), Some(VecLikeType::Subr));
    assert_eq!(original.as_subr_id(), Some(intern("car")));
}
