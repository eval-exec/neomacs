//! Portable dumper (pdump) for NeoVM.
//!
//! File-backed dumps use an mmap image container: a fixed header, build
//! fingerprint, section table, checksum, and private mmap on load.  File load
//! rebuilds the evaluator from explicit sections; it no longer deserializes a
//! monolithic `DumpContextState` payload.
//!
//! File format:
//! ```text
//! [fixed mmap image header]
//! [section table]
//! [symbol/runtime manager/root sections]
//! [heap/relocation sections]
//! ```
//!
//! Hosts without file-backed memory maps use a separate portable snapshot of
//! the pointer-free logical state. The native mmap image remains optimized for
//! direct mapping and is never treated as a cross-target wire format.

pub(crate) mod autoloads_image;
pub(crate) mod buffer_image;
pub(crate) mod charset_image;
pub(crate) mod coding_system_image;
pub mod convert;
pub(crate) use convert::{
    materialize_and_publish_stub, stub_interactive_probe, stub_params_required_only,
};
pub(crate) mod face_image;
pub(crate) mod image_format;
pub(crate) mod mapped_heap;
std::cfg_select! {
    target_family = "wasm" => {
        #[path = "mmap_image_portable.rs"]
        pub(crate) mod mmap_image;
    }
    _ => { pub(crate) mod mmap_image; }
}
pub(crate) mod obarray_image;
pub(crate) mod object_extra;
pub(crate) mod object_starts;
pub(crate) mod object_value_codec;
mod portable_snapshot;
pub use portable_snapshot::{
    PORTABLE_RUNTIME_IMAGE_ENV, dump_portable_snapshot_to_file, encode_portable_snapshot,
    load_from_portable_snapshot,
};
pub(crate) mod roots_image;
pub mod runtime;
pub(crate) mod runtime_managers_image;
pub(crate) mod symbol_table_image;
pub mod types;
pub(crate) mod value_fixups;

use std::path::Path;
use std::sync::OnceLock;

use self::convert::*;
use self::mmap_image::{DumpSectionKind, ImageSection};
use self::runtime::*;
use self::types::DumpContextState;
use crate::emacs_core::charset::{
    CharsetRegistrySnapshot, restore_charset_registry, snapshot_charset_registry,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::fontset::{
    FontsetRegistrySnapshot, restore_fontset_registry, snapshot_fontset_registry,
};
#[cfg(test)]
use crate::emacs_core::value;

// Phase 21 bump: phase 16 introduced an explicit dump-local symbol table,
// phase 17 fixed the on-disk `DumpSymbolData` layout, phase 18 stores subr
// names as dump-local name atoms instead of dump-local symbol slots,
// phase 19 stores `loads_in_progress` as Lisp strings instead of UTF-8 paths,
// phase 20 was the Phase H SymbolValue deletion (in-memory only, no wire-format
// change needed at that time), and phase 21 redesigns `DumpSymbolData` to
// drop the legacy `name`/`value`/`symbol_value`/`special`/`constant` fields
// and encode redirect + flags + value cell directly.  Old dumps are discarded
// and regenerated (no backward compatibility required per project memory S105).
// v22: LispSymbol::plist flipped to Value cons list (DumpSymbolData::plist is now DumpValue).
// v23: LispSymbol::function flipped to Value with NIL sentinel (DumpSymbolData::function is now DumpValue).
// v25: see commit history (slot/local_var_alist round-trip).
// v26: marker GNU-parity refactor (T10). DumpMarker now carries `bytepos`/`charpos`
//   directly (the legacy `position: Option<i64>` field is gone) and `DumpBuffer.markers`
//   is `Vec<DumpMarker>` walked in head→tail chain order. The retired
//   `DumpMarkerEntry` shape (per-buffer flat tuple) is no longer accepted; old v25
//   dumps fail with `UnsupportedVersion` and are regenerated from scratch
//   per the project's "no backward compat for pdump" policy.
// v27: GNU low-tag parity: fixnums use tags 010/110, cons is 011,
//   vectorlike is 101, float is 111. `Qunbound` is a noncanonical symbol
//   value, and subrs are PVEC_SUBR-like vectorlike objects instead of
//   Neomacs-only immediate tag 111 values.
// v28: Heap string bytes move out of the runtime-state bincode payload into
//   the mmap heap section, matching GNU pdumper's hot string header plus cold
//   string data split.
// v29: Vector, record, lambda, and macro slot arrays are installed from
//   mmap heap spans during file pdump load.
// v30: Cons cells are loaded as real tagged pointers into the mmap heap image
//   with external GC mark bits, matching GNU pdumper's dumped-object marking
//   model for conses instead of reconstructing them as fresh heap allocations.
// v31: Float DumpValues now refer to heap objects, and those FloatObj payloads
//   load as real tagged pointers into the mmap heap image with external GC mark
//   bits, matching GNU pdumper's `dump_float` object-shaped cold dump.
// v32: Vector, record, lambda, and macro object headers load from the mmap heap
//   image too; their slot payloads remain in mapped slot spans, mirroring GNU's
//   vectorlike header + Lisp_Object content dump shape.
// v33: String object headers also load from the mmap heap image; string bytes
//   stay in mapped byte spans and text-property roots are marked externally.
// v34: Marker and overlay vectorlike object headers load from the mmap heap
//   image and use the same external mapped-object mark bits as other dumped
//   vectorlike objects.
// v35: mmap relocations carry a tagged-value addend, and dump writes raw cons,
//   float, and vector slot payload contents into the heap image instead of
//   reserving empty arenas for load-time reconstruction.
// v36: Dump-local symbol interner metadata moves out of RuntimeState bincode
//   and into a fixed-layout SymbolTable mmap section.
// v37: Mapped heap object/slot spans move out of RuntimeState bincode. File
//   load rebuilds them from the dumped object list and the fixed heap-image
//   layout algorithm instead of deserializing five span vectors.
// v38: DumpTaggedHeap.objects moved out of RuntimeState bincode and into an
//   explicit heap/value-tag section. This intermediate section was superseded
//   by v47's ObjectStarts/ObjectExtra/ValueRelocations layout.
// v39: Obarray symbol state moves out of RuntimeState bincode and into a
//   fixed-layout Obarray mmap section.
// v40: Top-level Lisp roots move out of RuntimeState bincode and into the
//   fixed-layout Roots mmap section.
// v41: Autoload manager state moves out of RuntimeState bincode and into a
//   fixed-layout Autoloads mmap section.
// v42: Charset registry state moves out of RuntimeState bincode and into the
//   fixed-layout CharsetRegistry mmap section.
// v43: Coding-system manager state moves out of RuntimeState bincode and into
//   the fixed-layout CodingSystems mmap section.
// v44: Lisp face table state moves out of RuntimeState bincode and into the
//   fixed-layout FaceTable mmap section.
// v45: Buffer manager state moves out of RuntimeState bincode and into the
//   fixed-layout Buffers mmap section.
// v46: The remaining runtime managers move out of RuntimeState bincode and
//   into the fixed-layout RuntimeManagers mmap section. File pdumps no longer
//   write or read RuntimeState.
// v47: The monolithic HeapObjects section is removed. HeapImage keeps raw
//   object bytes, ObjectStarts records exact mapped object starts/types, and
//   ValueRelocations patches mapped value words whose value cannot be a plain
//   heap-image pointer relocation.
// v48: HeapImage pointer relocations match GNU pdumper's dump_reloc shape more
//   closely: the dump word stores the target heap offset, while compact
//   relocation metadata stores only the location offset and Lisp tag.
// v49: Symbol ValueRelocations use the same shape: the dump word stores the
//   dump-local symbol slot, and compact metadata stores only aligned location
//   offset plus fixup type. Full DumpValue fixups remain only for rare runtime
//   values such as subrs and non-mapped heap objects.
// v50: ObjectStarts span offsets and lengths use checked 32-bit dump offsets,
//   matching GNU pdumper's dump_off instead of serializing every field as u64.
// v51: ObjectExtra no longer duplicates mapped vectorlike slot counts; mapped
//   ObjectStarts slot spans are the source of truth, like GNU's mapped objects.
// v52: ObjectExtra string metadata uses checked 32-bit dump fields for sizes,
//   spans, and text-property run bounds, matching GNU pdumper's dump_off model.
// v53: ObjectExtra is sparse: category-A mapped objects come from ObjectStarts
//   and mapped heap headers instead of one semantic descriptor tag per object.
// v55: Runtime VecLikeType discriminants now mirror GNU `enum pvec_type` for
//   all shared vectorlike tags; old mmap heap headers used Neomacs-local codes.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
// v56: preserve native Boolean (`Lisp_Boolfwd`) symbol storage and rebuild
// its stable descriptor when loading the obarray image.
// v57: same for integer (`Lisp_Intfwd`) symbol storage, now that every GNU
// `DEFVAR_INT` variable is forwarded rather than a plain obarray cell.
// v58: a `Localized` symbol records WHICH forwarder `make_blv` copied into its
// BLV (`src/data.c:2112-2140`), so a `DEFVAR_BOOL`/`DEFVAR_INT` variable Lisp
// made buffer-local gets an equivalent descriptor back from the image instead
// of from a hand-maintained name list.
// v59: `DEFVAR_LISP` and `DEFVAR_KBOARD` names are `SYMBOL_FORWARDED` too, so
// the image carries their slot value and rebuilds the descriptor -- the same
// contract v56 and v57 gave Boolean and integer slots, and the same two new
// kinds for a BLV that `make_blv` copied one of them into.
const FORMAT_VERSION: u32 = 59;

const FINGERPRINT_PLACEHOLDER: [u8; 32] = *b"NEOMACS_PDUMP_FINGERPRINT_SLOT!!";

#[repr(C)]
struct ExecutableFingerprintRecord {
    magic_start: [u8; 16],
    fingerprint: [u8; 32],
    magic_end: [u8; 16],
}

#[used]
static NEOMACS_PDUMP_FINGERPRINT_RECORD: ExecutableFingerprintRecord =
    ExecutableFingerprintRecord {
        magic_start: *b"NEOMACS-FP-START",
        fingerprint: FINGERPRINT_PLACEHOLDER,
        magic_end: *b"NEOMACS-FP-END!!",
    };

pub fn fingerprint_hex() -> &'static str {
    static HEX: OnceLock<String> = OnceLock::new();
    HEX.get_or_init(|| hex_string(&fingerprint_bytes()))
}

fn fingerprint_bytes() -> [u8; 32] {
    unsafe {
        std::ptr::read_volatile(std::ptr::addr_of!(
            NEOMACS_PDUMP_FINGERPRINT_RECORD.fingerprint
        ))
    }
}

fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02X}");
    }
    out
}

/// Errors from dump/load operations.
#[derive(Debug)]
pub enum DumpError {
    Io(std::io::Error),
    BadMagic,
    UnsupportedVersion(u32),
    FingerprintMismatch { expected: String, found: String },
    ChecksumMismatch,
    ImageFormatError(String),
    SerializationError(String),
    DeserializationError(String),
}

impl std::fmt::Display for DumpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DumpError::Io(e) => write!(f, "I/O error: {e}"),
            DumpError::BadMagic => write!(f, "not a valid pdump file (bad magic)"),
            DumpError::UnsupportedVersion(v) => write!(f, "unsupported pdump version {v}"),
            DumpError::FingerprintMismatch { expected, found } => write!(
                f,
                "pdump fingerprint mismatch (expected {expected}, found {found})"
            ),
            DumpError::ChecksumMismatch => write!(f, "pdump checksum mismatch (corrupted file)"),
            DumpError::ImageFormatError(s) => write!(f, "pdump image format error: {s}"),
            DumpError::SerializationError(s) => write!(f, "serialization error: {s}"),
            DumpError::DeserializationError(s) => write!(f, "deserialization error: {s}"),
        }
    }
}

impl std::error::Error for DumpError {}

impl From<std::io::Error> for DumpError {
    fn from(e: std::io::Error) -> Self {
        DumpError::Io(e)
    }
}

fn empty_lisp_string() -> types::DumpLispString {
    types::DumpLispString {
        data: Vec::new(),
        size: 0,
        size_byte: 0,
    }
}

fn empty_context_state() -> DumpContextState {
    DumpContextState {
        symbol_table: types::DumpSymbolTable {
            names: Vec::new(),
            symbols: Vec::new(),
        },
        tagged_heap: types::DumpTaggedHeap {
            objects: Vec::new(),
            mapped_cons: Vec::new(),
            mapped_floats: Vec::new(),
            mapped_strings: Vec::new(),
            mapped_veclikes: Vec::new(),
            mapped_slots: Vec::new(),
        },
        obarray: types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
        dynamic: Vec::new(),
        lexenv: types::DumpValue::Nil,
        features: Vec::new(),
        require_stack: Vec::new(),
        loads_in_progress: Vec::new(),
        buffers: buffer_image::empty_buffer_manager(),
        autoloads: autoloads_image::empty_autoloads(),
        custom: types::DumpCustomManager {
            auto_buffer_local_syms: Vec::new(),
            auto_buffer_local: Vec::new(),
        },
        modes: types::DumpModeRegistry {
            major_modes: Vec::new(),
            minor_modes: Vec::new(),
            buffer_major_modes: Vec::new(),
            buffer_minor_modes: Vec::new(),
            global_minor_modes: Vec::new(),
            auto_mode_alist_lisp: Vec::new(),
            auto_mode_alist: Vec::new(),
            custom_variables: Vec::new(),
            custom_groups: Vec::new(),
            fundamental_mode: types::DumpValue::Nil,
        },
        coding_systems: coding_system_image::empty_coding_system_manager(),
        charset_registry: charset_image::empty_charset_registry(),
        fontset_registry: types::DumpFontsetRegistry {
            ordered_names_lisp: Vec::new(),
            alias_to_name_lisp: Vec::new(),
            fontsets_lisp: Vec::new(),
            ordered_names: Vec::new(),
            alias_to_name: Vec::new(),
            fontsets: Vec::new(),
            generation: 0,
        },
        face_table: face_image::empty_face_table(),
        abbrevs: types::DumpAbbrevManager {
            tables_syms: Vec::new(),
            tables: Vec::new(),
            global_table_sym: None,
            global_table_name: empty_lisp_string(),
            abbrev_mode: false,
        },
        interactive: types::DumpInteractiveRegistry { specs: Vec::new() },
        rectangle: types::DumpRectangleState { killed: Vec::new() },
        standard_syntax_table: types::DumpValue::Nil,
        syntax_code_objects: types::DumpValue::Nil,
        standard_category_table: types::DumpValue::Nil,
        current_local_map: types::DumpValue::Nil,
        current_global_map: types::DumpValue::Nil,
        kmacro: types::DumpKmacroManager {
            current_macro: Vec::new(),
            last_macro: None,
            macro_ring: Vec::new(),
            counter: 0,
            counter_format_lisp: None,
            counter_format: None,
        },
        registers: types::DumpRegisterManager {
            registers: Vec::new(),
        },
        bookmarks: types::DumpBookmarkManager {
            bookmarks_lisp: Vec::new(),
            bookmarks: Vec::new(),
            recent: Vec::new(),
        },
        watchers: types::DumpVariableWatcherList {
            watchers: Vec::new(),
        },
    }
}

/// Thread-local semantic runtime state that must be restored when switching
/// back from a cloned evaluator to the live evaluator on the same thread.
#[derive(Clone, Debug)]
pub struct ActiveRuntimeSnapshot {
    charset_registry: CharsetRegistrySnapshot,
    fontset_registry: FontsetRegistrySnapshot,
}

struct RestoreCleanup;

impl Drop for RestoreCleanup {
    fn drop(&mut self) {
        finish_load_interner();
    }
}

/// Serialize the evaluator state to a pdump file.
pub fn dump_to_file(eval: &Context, path: &Path) -> Result<(), DumpError> {
    let mut state = dump_evaluator(eval);
    let symbol_table_payload = symbol_table_image::symbol_table_section_bytes(&state.symbol_table)?;
    let heap_payload = mapped_heap::extract_mapped_heap_payloads(&mut state);
    let object_starts_payload = object_starts::build_object_starts(&state.tagged_heap)?;
    let object_extra_payload = object_extra::build_object_extra(
        &state.tagged_heap.objects,
        &state.tagged_heap.mapped_slots,
    )?;
    let value_fixups_payload =
        value_fixups::value_fixups_section_bytes(&heap_payload.value_fixups)?;
    let obarray_payload = obarray_image::obarray_section_bytes(&state.obarray)?;
    let charset_payload = charset_image::charset_section_bytes(&state.charset_registry)?;
    let coding_system_payload =
        coding_system_image::coding_system_section_bytes(&state.coding_systems)?;
    let face_payload = face_image::face_table_section_bytes(&state.face_table)?;
    let buffer_payload = buffer_image::buffer_manager_section_bytes(&state.buffers)?;
    let autoloads_payload = autoloads_image::autoloads_section_bytes(&state.autoloads)?;
    let runtime_managers_payload = runtime_managers_image::runtime_managers_section_bytes(
        &runtime_managers_image::RuntimeManagersState::from_context_state(&state),
    )?;
    let roots_payload = roots_image::roots_section_bytes(&roots_image::DumpRootState {
        dynamic: state.dynamic.clone(),
        lexenv: state.lexenv.clone(),
        features: state.features.clone(),
        require_stack: state.require_stack.clone(),
        loads_in_progress: state.loads_in_progress.clone(),
        standard_syntax_table: state.standard_syntax_table.clone(),
        syntax_code_objects: state.syntax_code_objects.clone(),
        standard_category_table: state.standard_category_table.clone(),
        current_local_map: state.current_local_map.clone(),
        current_global_map: state.current_global_map.clone(),
    })?;
    let relocation_payload = mmap_image::relocation_section_bytes(&heap_payload.relocations);

    let mut sections = Vec::with_capacity(
        11 + usize::from(!heap_payload.bytes.is_empty())
            + usize::from(!relocation_payload.is_empty()),
    );
    sections.push(ImageSection {
        kind: DumpSectionKind::SymbolTable,
        flags: 0,
        bytes: &symbol_table_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::ObjectStarts,
        flags: 0,
        bytes: &object_starts_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::ObjectExtra,
        flags: 0,
        bytes: &object_extra_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::Obarray,
        flags: 0,
        bytes: &obarray_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::CharsetRegistry,
        flags: 0,
        bytes: &charset_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::CodingSystems,
        flags: 0,
        bytes: &coding_system_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::FaceTable,
        flags: 0,
        bytes: &face_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::Buffers,
        flags: 0,
        bytes: &buffer_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::Roots,
        flags: 0,
        bytes: &roots_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::Autoloads,
        flags: 0,
        bytes: &autoloads_payload,
    });
    sections.push(ImageSection {
        kind: DumpSectionKind::RuntimeManagers,
        flags: 0,
        bytes: &runtime_managers_payload,
    });
    if !heap_payload.bytes.is_empty() {
        sections.push(ImageSection {
            kind: DumpSectionKind::HeapImage,
            flags: 0,
            bytes: &heap_payload.bytes,
        });
    }
    if !relocation_payload.is_empty() {
        sections.push(ImageSection {
            kind: DumpSectionKind::Relocations,
            flags: 0,
            bytes: &relocation_payload,
        });
    }
    if !value_fixups_payload.is_empty() {
        sections.push(ImageSection {
            kind: DumpSectionKind::ValueRelocations,
            flags: 0,
            bytes: &value_fixups_payload,
        });
    }

    mmap_image::write_image(path, &sections)
}

/// Load evaluator state from a pdump file.
///
/// This reconstructs a full `Context` from explicit mmap sections, setting up
/// thread-local pointers and resetting caches.
pub fn load_from_dump(path: &Path) -> Result<Context, DumpError> {
    let load_start = std::time::Instant::now();
    let mut image = mmap_image::load_image(path)?;
    image.apply_relocations()?;
    let mut eval = match load_from_mapped_image(&mut image) {
        Ok(eval) => eval,
        Err(err) => {
            // A failed load may already have installed process-lifetime
            // `LispString::borrowed_alias` symbol names into the GLOBAL
            // interner (`intern_dump_lisp_string`). Unmapping now would
            // leave those registry keys dangling — the fallback loadup's
            // very first `intern` memcmps freed memory and SEGVs. Leak the
            // mapping instead: failure is rare (version mismatch or a
            // corrupt file, once per process), and the aliases were
            // promised the image's bytes for the process lifetime.
            std::mem::forget(image);
            return Err(err);
        }
    };
    record_loaded_dump(path, load_start.elapsed());
    mark_after_pdump_load_hook_pending(&mut eval);
    eval.install_pdump_image(image);
    Ok(eval)
}

fn load_from_mapped_image(image: &mut mmap_image::LoadedMmapImage) -> Result<Context, DumpError> {
    let mut state = empty_context_state();

    let _cleanup = RestoreCleanup;
    let symbol_table_payload = image
        .section(DumpSectionKind::SymbolTable)
        .ok_or_else(|| DumpError::ImageFormatError("missing symbol-table section".into()))?;
    symbol_table_image::load_symbol_table_section(symbol_table_payload)?;
    let object_starts_payload = image
        .section(DumpSectionKind::ObjectStarts)
        .ok_or_else(|| DumpError::ImageFormatError("missing object-starts section".into()))?;
    let spans = object_starts::load_object_starts(object_starts_payload)?;
    let mapped_heap = image
        .section_mut_ptr(DumpSectionKind::HeapImage)
        .map(|(ptr, len)| unsafe { mapped_heap::MappedHeapView::from_raw_parts(ptr, len, true) });
    let object_extra_payload = image
        .section(DumpSectionKind::ObjectExtra)
        .ok_or_else(|| DumpError::ImageFormatError("missing object-extra section".into()))?;
    let object_descriptors =
        object_extra::load_file_object_descriptors(object_extra_payload, &spans, mapped_heap)?;
    if spans.len() != object_descriptors.len() {
        return Err(DumpError::ImageFormatError(format!(
            "object-starts count {} does not match object-extra count {}",
            spans.len(),
            object_descriptors.len()
        )));
    }
    let obarray_payload = image
        .section(DumpSectionKind::Obarray)
        .ok_or_else(|| DumpError::ImageFormatError("missing obarray section".into()))?;
    state.obarray = obarray_image::load_obarray_section(obarray_payload)?;
    let charset_payload = image
        .section(DumpSectionKind::CharsetRegistry)
        .ok_or_else(|| DumpError::ImageFormatError("missing charset-registry section".into()))?;
    state.charset_registry = charset_image::load_charset_section(charset_payload)?;
    let coding_system_payload = image
        .section(DumpSectionKind::CodingSystems)
        .ok_or_else(|| DumpError::ImageFormatError("missing coding-systems section".into()))?;
    state.coding_systems = coding_system_image::load_coding_system_section(coding_system_payload)?;
    let face_payload = image
        .section(DumpSectionKind::FaceTable)
        .ok_or_else(|| DumpError::ImageFormatError("missing face-table section".into()))?;
    state.face_table = face_image::load_face_table_section(face_payload)?;
    let buffer_payload = image
        .section(DumpSectionKind::Buffers)
        .ok_or_else(|| DumpError::ImageFormatError("missing buffers section".into()))?;
    state.buffers = buffer_image::load_buffer_manager_section(buffer_payload)?;
    let roots_payload = image
        .section(DumpSectionKind::Roots)
        .ok_or_else(|| DumpError::ImageFormatError("missing roots section".into()))?;
    let roots = roots_image::load_roots_section(roots_payload)?;
    state.dynamic = roots.dynamic;
    state.lexenv = roots.lexenv;
    state.features = roots.features;
    state.require_stack = roots.require_stack;
    state.loads_in_progress = roots.loads_in_progress;
    state.standard_syntax_table = roots.standard_syntax_table;
    state.syntax_code_objects = roots.syntax_code_objects;
    state.standard_category_table = roots.standard_category_table;
    state.current_local_map = roots.current_local_map;
    state.current_global_map = roots.current_global_map;
    let autoloads_payload = image
        .section(DumpSectionKind::Autoloads)
        .ok_or_else(|| DumpError::ImageFormatError("missing autoloads section".into()))?;
    state.autoloads = autoloads_image::load_autoloads_section(autoloads_payload)?;
    let runtime_managers_payload = image
        .section(DumpSectionKind::RuntimeManagers)
        .ok_or_else(|| DumpError::ImageFormatError("missing runtime-managers section".into()))?;
    runtime_managers_image::load_runtime_managers_section(runtime_managers_payload)?
        .install_into(&mut state);

    let value_fixups_section = image.section(DumpSectionKind::ValueRelocations);

    let eval = reconstruct_evaluator_after_symbol_table_with_tagged_heap_parts(
        &state,
        object_descriptors,
        spans,
        mapped_heap,
        value_fixups_section,
    )?;
    drop(_cleanup);
    Ok(eval)
}

/// Clone a live evaluator through the pdump conversion pipeline.
///
/// This gives bootstrap/load code an isolated working evaluator with the same
/// logical runtime state, without sharing heap objects that can be mutated
/// during eager macroexpansion.
pub fn snapshot_evaluator(eval: &Context) -> DumpContextState {
    dump_evaluator(eval)
}

/// Snapshot an evaluator after activating its thread-local runtime bindings.
///
/// Use this entry point when multiple `Context`s may share the current thread.
/// The pdump conversion pipeline relies on thread-local tagged-heap state, so
/// the source evaluator must be active before we walk its heap-backed values.
pub fn snapshot_active_evaluator(eval: &mut Context) -> DumpContextState {
    eval.setup_thread_locals();
    dump_evaluator(eval)
}

/// Snapshot thread-local semantic runtime registries for the active evaluator.
///
/// Cloning an evaluator through pdump reconstructs these registries for the
/// cloned heap. Callers that later switch the current thread back to the live
/// evaluator must restore this snapshot as part of runtime reactivation.
pub fn snapshot_active_runtime(eval: &mut Context) -> ActiveRuntimeSnapshot {
    eval.setup_thread_locals();
    ActiveRuntimeSnapshot {
        charset_registry: snapshot_charset_registry(),
        fontset_registry: snapshot_fontset_registry(),
    }
}

/// Reactivate a live evaluator after using a cloned evaluator on the same
/// thread, restoring thread-local semantic registries alongside heap state.
pub fn restore_active_runtime(eval: &mut Context, snapshot: &ActiveRuntimeSnapshot) {
    eval.setup_thread_locals();
    restore_charset_registry(snapshot.charset_registry.clone());
    restore_fontset_registry(snapshot.fontset_registry.clone());
    eval.sync_thread_runtime_bindings();
    eval.sync_current_thread_buffer_state();
}

/// Reconstruct an evaluator from a previously captured in-memory pdump snapshot.
pub fn restore_snapshot(state: &DumpContextState) -> Result<Context, DumpError> {
    reconstruct_evaluator(state, None)
}

/// Clone a live evaluator through the pdump conversion pipeline.
///
/// Prefer `snapshot_evaluator` + `restore_snapshot` when cloning the same
/// template repeatedly; that avoids rebuilding the intermediate dump state.
pub fn clone_evaluator(eval: &Context) -> Result<Context, DumpError> {
    restore_snapshot(&snapshot_evaluator(eval))
}

/// Clone an evaluator after activating its thread-local runtime bindings.
///
/// Use this when cloning from a live runtime that shares the current thread
/// with other `Context`s.
pub fn clone_active_evaluator(eval: &mut Context) -> Result<Context, DumpError> {
    restore_snapshot(&snapshot_active_evaluator(eval))
}

/// Reconstruct an `Context` from deserialized dump state.
fn reconstruct_evaluator(
    state: &DumpContextState,
    mapped_heap: Option<mapped_heap::MappedHeapView>,
) -> Result<Context, DumpError> {
    // 1. Reconstruct the dump-local symbol table before any values that refer
    // to dump-local `DumpSymId`s are loaded.
    let _cleanup = RestoreCleanup;
    load_symbol_table(&state.symbol_table)?;
    let eval = reconstruct_evaluator_after_symbol_table(state, mapped_heap, &[])?;
    drop(_cleanup);
    Ok(eval)
}

fn reconstruct_evaluator_after_symbol_table(
    state: &DumpContextState,
    mapped_heap: Option<mapped_heap::MappedHeapView>,
    value_fixups: &[value_fixups::RawValueFixup],
) -> Result<Context, DumpError> {
    let decoder = LoadDecoder::new_with_mapped_heap_and_fixups(
        &state.tagged_heap,
        mapped_heap,
        value_fixups.to_vec(),
    );
    reconstruct_evaluator_after_symbol_table_with_decoder(state, decoder)
}

fn reconstruct_evaluator_after_symbol_table_with_tagged_heap_parts(
    state: &DumpContextState,
    object_descriptors: object_extra::FileObjectDescriptors,
    spans: object_starts::LoadedSpans<'_>,
    mapped_heap: Option<mapped_heap::MappedHeapView>,
    value_fixups_section: Option<&[u8]>,
) -> Result<Context, DumpError> {
    let decoder = LoadDecoder::from_file_descriptors_and_spans_with_mapped_heap_and_fixups(
        object_descriptors,
        spans,
        mapped_heap,
        Vec::new(),
    );
    reconstruct_evaluator_after_symbol_table_with_decoder_and_value_fixups(
        state,
        decoder,
        value_fixups_section,
    )
}

fn reconstruct_evaluator_after_symbol_table_with_decoder(
    state: &DumpContextState,
    decoder: LoadDecoder<'_>,
) -> Result<Context, DumpError> {
    reconstruct_evaluator_after_symbol_table_with_decoder_and_value_fixups(state, decoder, None)
}

fn reconstruct_evaluator_after_symbol_table_with_decoder_and_value_fixups(
    state: &DumpContextState,
    mut decoder: LoadDecoder<'_>,
    value_fixups_section: Option<&[u8]>,
) -> Result<Context, DumpError> {
    // 2. Reconstruct the tagged heap before any heap-backed value/object loads
    // so tagged dump references can resolve directly to live tagged objects.
    let mut tagged_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
    crate::tagged::gc::set_tagged_heap(&mut tagged_heap);
    decoder.preload_tagged_heap_with_value_fixup_section(value_fixups_section)?;

    // 3. Reset thread-local runtime caches before replaying semantic state.
    reset_runtime_for_new_heap(HeapResetMode::PdumpRestore);

    // 4b. Restore thread-local registries whose contents are semantic runtime
    // state, not disposable caches.
    load_charset_registry(&mut decoder, &state.charset_registry);
    load_fontset_registry(&state.fontset_registry);

    // 5. Reconstruct all subsystems
    let obarray = load_obarray(&mut decoder, &state.obarray)?;
    let lexenv = decoder.load_value(&state.lexenv);
    let features: Vec<_> = state.features.iter().map(load_sym_id).collect();
    let require_stack: Vec<_> = state.require_stack.iter().map(load_sym_id).collect();
    let loads_in_progress: Vec<_> = state
        .loads_in_progress
        .iter()
        .map(load_lisp_string)
        .collect();
    let selected_global_map =
        super::keymap::SelectedGlobalMap::from_dump(decoder.load_value(&state.current_global_map))
            .ok_or_else(|| {
                DumpError::DeserializationError(
                    "selected global map is neither nil nor a resolved keymap".into(),
                )
            })?;

    let mut eval = Context::from_dump(
        tagged_heap,
        obarray,
        lexenv,
        features,
        require_stack,
        loads_in_progress,
        load_buffer_manager(&mut decoder, &state.buffers),
        load_autoload_manager(&mut decoder, &state.autoloads),
        load_custom_manager(&state.custom),
        load_mode_registry(&mut decoder, &state.modes),
        load_coding_system_manager(&mut decoder, &state.coding_systems),
        load_face_table(&mut decoder, &state.face_table),
        load_abbrev_manager(&state.abbrevs),
        load_interactive_registry(&mut decoder, &state.interactive),
        load_rectangle(&state.rectangle),
        decoder.load_value(&state.standard_syntax_table),
        decoder.load_value(&state.syntax_code_objects),
        decoder.load_value(&state.standard_category_table),
        decoder.load_value(&state.current_local_map),
        selected_global_map,
        load_kmacro(&mut decoder, &state.kmacro),
        load_register_manager(&mut decoder, &state.registers),
        load_bookmark_manager(&state.bookmarks),
        load_watcher_list(&mut decoder, &state.watchers),
    );

    // Phase 10E follow-up: re-install BUFFER_OBJFWD forwarders.
    // `pdump::convert::load_symbol_data` leaves SymbolValue::Forwarded
    // entries at redirect=Plainval (the descriptor pointer is a
    // 'static reference and is rebuilt from BUFFER_SLOT_INFO at
    // install time, so the dump never carried it). Without this
    // loop, every per-buffer C-slot variable behaves like a plain
    // global after a snapshot restore, breaking writes routed via
    // `Buffer::set_buffer_local`. Mirrors the matching loop in
    // `Context::new_inner` and `finalize_cached_bootstrap_eval`.
    {
        use crate::buffer::buffer::BUFFER_SLOT_INFO;
        use crate::emacs_core::forward::alloc_buffer_objfwd;
        use crate::emacs_core::intern::intern;
        let obarray = eval.obarray_mut();
        for info in BUFFER_SLOT_INFO {
            if !info.install_as_forwarder {
                continue;
            }
            let id = intern(info.name);
            let fwd = alloc_buffer_objfwd(
                info.offset.as_u16(),
                info.local_flags_idx,
                info.predicate,
                info.default.to_value(),
            );
            obarray.install_buffer_objfwd(id, fwd);
        }
    }

    decoder.discard_restored_file_object_descriptors();
    Ok(eval)
}

fn mark_after_pdump_load_hook_pending(eval: &mut Context) {
    eval.after_pdump_load_hook_pending = true;
}

pub(crate) fn take_after_pdump_load_hook_pending(eval: &mut Context) -> bool {
    let pending = eval.after_pdump_load_hook_pending;
    eval.after_pdump_load_hook_pending = false;
    pending
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
