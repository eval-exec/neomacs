use neovm_core::tagged::header::{SubrDispatchKind, SubrInteractivity};
use neovm_core::tagged::value::{StaticSubrEntrySync, update_static_subr_object_entry};
use neovm_core::emacs_core::intern::SymId;
fn forged(id: SymId) {
    update_static_subr_object_entry::<fn()>(id, None, 0, None,
        SubrDispatchKind::Builtin, SubrInteractivity::NonInteractive, StaticSubrEntrySync::Locked);
}
fn main() {}
