//! Numeric AOT ABI tag pin and append-only history (lead 2026-11-09 #882;
//! peer A1c condition (5)).
//!
//! An AOT entry symbol and the preload manifest carry `ABI_TAG`, a salted
//! fingerprint of the JIT/AOT contract.  neovm-eval's own `aot_test`
//! consistency assertions prove the tag round-trips through the artifact,
//! but only a NUMERIC pin catches a forgotten bump after a calling
//! convention, frame-layout or packing change: a stale `.so` would
//! otherwise be accepted and run the old frame protocol.  This guard runs
//! in the core harness (the eval lib-test target is not standalone
//! runnable), reads `ABI_TAG_VERSION` from the production source, and
//! requires:
//!   - the recorded history is strictly increasing (hence unique), and
//!   - its last entry equals the source's `ABI_TAG_VERSION`.
//!
//! ANY change to the JIT/AOT calling convention, frame layout or packing
//! MUST bump `ABI_TAG_VERSION` in
//! `crates/neovm-eval/src/emacs_core/runtime/jit/aot.rs` and append the new
//! value to `ABI_TAG_HISTORY` here.

use std::path::PathBuf;

/// `ABI_TAG_VERSION` values in landing order.  Append-only.
const ABI_TAG_HISTORY: &[u32] = &[52, 53, 54];

const DECL: &str = "const ABI_TAG_VERSION: u32 = ";

fn aot_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../neovm-eval/src/emacs_core/runtime/jit/aot.rs");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn aot_abi_tag_guard_pins_the_version_and_requires_an_append_only_history() {
    for pair in ABI_TAG_HISTORY.windows(2) {
        assert!(
            pair[0] < pair[1],
            "ABI_TAG_HISTORY must be strictly increasing and unique: {ABI_TAG_HISTORY:?}"
        );
    }
    let src = aot_source();
    let line = src
        .lines()
        .find(|line| line.trim_start().starts_with(DECL))
        .unwrap_or_else(|| panic!("no `{DECL}N;` declaration in the AOT source"));
    let digits = line.trim_start()[DECL.len()..].trim_end_matches(';').trim();
    let version: u32 = digits
        .parse()
        .unwrap_or_else(|error| panic!("parse ABI_TAG_VERSION from {digits:?}: {error}"));
    assert_eq!(
        version,
        *ABI_TAG_HISTORY.last().expect("history is non-empty"),
        "ABI_TAG_VERSION is {version}, but ABI_TAG_HISTORY ends at {:?}: a JIT/AOT \
         calling-convention, frame-layout or packing change must bump the tag and \
         append the new value to the history",
        ABI_TAG_HISTORY.last()
    );
}

/// `lower_car_cdr` and `emit_plain_slot_address` bake cons and vector field
/// offsets into AOT code. `compute_abi_tag` must salt those layouts, or a
/// `NEOVM_AOT_DIR` unit from another build is served and reads the wrong words.
#[test]
fn abi_tag_salts_heap_layouts_baked_into_aot_code() {
    let src = aot_source();
    let start = src
        .find("const fn compute_abi_tag()")
        .expect("compute_abi_tag");
    let body = &src[start..];
    let end = body.find("\n}\n").expect("compute_abi_tag closing brace");
    let body = &body[..end];
    for needle in [
        "CONS_CAR_OFFSET",
        "CONS_CDR_OFFSET",
        "TAG_MASK",
        "TAG_CONS",
        "TAG_STRING",
        "TAG_VECLIKE",
        "offset_of!(neovm_value::tagged::header::VectorObj, data)",
        "offset_of!(neovm_value::tagged::header::RecordObj, data)",
        "offset_of!(neovm_value::tagged::header::StringObj, data)",
        "offset_of!(neovm_value::tagged::header::VecLikeHeader, type_tag)",
        "size_of::<neovm_value::tagged::header::GcHeader>()",
        "offset_of!(neovm_value::tagged::header::GcHeader, marked)",
        "offset_of!(neovm_value::tagged::header::GcHeader, kind)",
        "offset_of!(neovm_value::tagged::header::GcHeader, tenured)",
        "offset_of!(neovm_value::tagged::header::GcHeader, remembered)",
        "offset_of!(neovm_value::tagged::header::GcHeader, flags)",
        "offset_of!(neovm_value::tagged::header::GcHeader, generation)",
        "offset_of!(neovm_value::tagged::header::GcHeader, next)",
    ] {
        assert!(
            body.contains(needle),
            "compute_abi_tag does not salt {needle}; a stale NEOVM_AOT_DIR unit would keep the old heap layout"
        );
    }
}
