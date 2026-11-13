use super::*;
use neovm_value::intern::intern;

/// A Bcall leaf is found through the builtin it names, and an opcode leaf
/// through no builtin: its answers are the opcode's (`Bnth` signals with the
/// tail, `Fnth` with the list), which a `Bcall` of the builtin must not get.
pub(crate) fn run_every_bcall_leaf_is_attached_to_its_registered_subr_fixture(
    make_context: fn() -> Context,
) {
    let ctx = make_context();
    for leaf in LEAVES {
        let sym = intern(leaf.name);
        assert!(
            crate::emacs_core::eval::lookup_global_subr_entry(sym).is_some(),
            "{} names a registered builtin",
            leaf.name
        );
        match leaf.shape {
            LeafShape::Bcall => {
                let attached = subr_leaf(sym).expect("a Bcall leaf is attached");
                assert!(std::ptr::eq(attached, leaf), "{}", leaf.name);
                assert!(
                    ctx.obarray.symbol_function_id(sym).is_some(),
                    "{} is fbound",
                    leaf.name
                );
            }
            LeafShape::Opcode => {
                assert!(subr_leaf(sym).is_none(), "{} is opcode-only", leaf.name);
            }
        }
    }
}
