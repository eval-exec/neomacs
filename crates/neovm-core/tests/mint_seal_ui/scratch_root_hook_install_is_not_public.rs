//! N9: the scratch-root hook installer is seam-only authority. At its
//! defining path (neovm-value) the installer is `pub unsafe fn`; a SAFE call
//! fails with E0133. The hook table below is inert; publishing it still
//! requires the unsafe contract.
use neovm_value::value::Value;
use neovm_value::value::scratch_bridge::{ScratchRootHooks, install_scratch_root_hooks};

fn save() -> usize {
    0
}

fn push(_: Value) {}

fn restore(_: usize) {}

static HOOKS: ScratchRootHooks = ScratchRootHooks::new(save, push, restore);

fn main() {
    let _ = install_scratch_root_hooks(&HOOKS);
}
