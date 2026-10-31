use super::*;

#[test]
fn context_collection_retains_obarray_seed_capacity_without_values() {
    let mut ctx = Context::new();
    let required = {
        let mut roots = Vec::new();
        ctx.obarray.trace_roots(&mut roots);
        roots.len()
    };
    assert!(required > 0);
    let mut first_allocation = None;
    for _ in 0..4 {
        ctx.gc_collect_exact();
        // SAFETY: this isolated Context has one stopped owner, with no Lisp
        // activation or concurrent marker while the scratch loan is inspected.
        let mut loan = unsafe { ctx.tagged_heap.root_seed_scratch() };
        let roots = loan.roots();
        assert!(
            roots.is_empty(),
            "collection must return no unrooted Values"
        );
        assert!(
            roots.capacity() >= required,
            "canonical Obarray roots must use the retained collector scratch"
        );
        match first_allocation {
            None => first_allocation = Some((roots.as_ptr(), roots.capacity())),
            Some(allocation) => assert_eq!((roots.as_ptr(), roots.capacity()), allocation),
        }
    }
}
