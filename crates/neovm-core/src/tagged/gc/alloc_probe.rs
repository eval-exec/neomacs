use super::{GcHeader, HeapObjectKind, TaggedHeap, VecLikeHeader, VecLikeType};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const N_BUCKETS: usize = 11;

// One declaration owns dense indices, report names, and fixed layouts.
// Adding a VecLike kind cannot silently shift only one of three parallel
// tables (the bug this replaces when PVEC_FONT was introduced).
macro_rules! allocation_kinds {
    ($( $variant:ident => ($name:literal, $ty:ty) ),+ $(,)?) => {
        #[derive(Clone, Copy)]
        #[repr(usize)]
        enum AllocKind { $( $variant, )+ Count }

        const N_KINDS: usize = AllocKind::Count as usize;
        pub(crate) const KIND_NAMES: [&str; N_KINDS] = [$( $name, )+];
        const FIXED_SIZES: [usize; N_KINDS] = [$( std::mem::size_of::<$ty>(), )+];
    };
}

allocation_kinds! {
    String => ("String", super::StringObj),
    Float => ("Float", super::FloatObj),
    Vector => ("Vector", super::VectorObj),
    Bignum => ("Bignum", super::BignumObj),
    Marker => ("Marker", super::MarkerObj),
    Overlay => ("Overlay", super::OverlayObj),
    Finalizer => ("Finalizer", super::FinalizerObj),
    SymbolWithPos => ("SymbolWithPos", super::SymbolWithPosObj),
    UserPtr => ("UserPtr", super::UserPtrObj),
    Process => ("Process", super::ProcessObj),
    Frame => ("Frame", super::FrameObj),
    Window => ("Window", super::WindowObj),
    Buffer => ("Buffer", super::BufferObj),
    HashTable => ("HashTable", super::HashTableObj),
    Obarray => ("Obarray", super::ObarrayObj),
    Terminal => ("Terminal", super::TerminalObj),
    WindowConfig => ("WindowConfig", super::RecordObj),
    Subr => ("Subr", super::SubrObj),
    Xwidget => ("Xwidget", super::XwidgetObj),
    XwidgetView => ("XwidgetView", super::XwidgetViewObj),
    ModuleFunction => ("ModuleFunction", super::ModuleFunctionObj),
    Sqlite => ("Sqlite", super::SqliteObj),
    Lambda => ("Lambda", super::LambdaObj),
    CharTable => ("CharTable", super::CharTableObj),
    SubCharTable => ("SubCharTable", super::SubCharTableObj),
    Record => ("Record", super::RecordObj),
    Font => ("Font", super::FontObj),
    Macro => ("Macro", super::MacroObj),
    ByteCode => ("ByteCode", super::ByteCodeObj),
    Timer => ("Timer", super::TimerObj),
    SurfaceHandle => ("SurfaceHandle", super::SurfaceObj),
    VideoHandle => ("VideoHandle", super::VideoObj),
}
/// Histogram bucket upper bounds (bytes).
pub(crate) const BUCKET_LABELS: [&str; N_BUCKETS] = [
    "<=16", "<=32", "<=64", "<=128", "<=256", "<=512", "<=1K", "<=4K", "<=16K", "<=64K", ">64K",
];

#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicU64 = AtomicU64::new(0);
#[allow(clippy::declare_interior_mutable_const)]
const ROW: [AtomicU64; N_BUCKETS] = [ZERO; N_BUCKETS];
static COUNTS: [[AtomicU64; N_BUCKETS]; N_KINDS] = [ROW; N_KINDS];
static TOTAL_BYTES: [AtomicU64; N_KINDS] = [ZERO; N_KINDS];
static PEAK_ADDR_SET: AtomicUsize = AtomicUsize::new(0);

fn kind_index(header: *const GcHeader) -> usize {
    let kind = match unsafe { (*header).kind } {
        HeapObjectKind::String => AllocKind::String,
        HeapObjectKind::Float => AllocKind::Float,
        HeapObjectKind::VecLike => match unsafe { (*(header as *const VecLikeHeader)).type_tag } {
            VecLikeType::Vector => AllocKind::Vector,
            VecLikeType::Bignum => AllocKind::Bignum,
            VecLikeType::Marker => AllocKind::Marker,
            VecLikeType::Overlay => AllocKind::Overlay,
            VecLikeType::Finalizer => AllocKind::Finalizer,
            VecLikeType::SymbolWithPos => AllocKind::SymbolWithPos,
            VecLikeType::UserPtr => AllocKind::UserPtr,
            VecLikeType::Process => AllocKind::Process,
            VecLikeType::Frame => AllocKind::Frame,
            VecLikeType::Window => AllocKind::Window,
            VecLikeType::Buffer => AllocKind::Buffer,
            VecLikeType::HashTable => AllocKind::HashTable,
            VecLikeType::Obarray => AllocKind::Obarray,
            VecLikeType::Terminal => AllocKind::Terminal,
            VecLikeType::WindowConfiguration => AllocKind::WindowConfig,
            VecLikeType::Subr => AllocKind::Subr,
            VecLikeType::Xwidget => AllocKind::Xwidget,
            VecLikeType::XwidgetView => AllocKind::XwidgetView,
            VecLikeType::ModuleFunction => AllocKind::ModuleFunction,
            VecLikeType::Sqlite => AllocKind::Sqlite,
            VecLikeType::Lambda => AllocKind::Lambda,
            VecLikeType::CharTable => AllocKind::CharTable,
            VecLikeType::SubCharTable => AllocKind::SubCharTable,
            VecLikeType::Record => AllocKind::Record,
            VecLikeType::Font => AllocKind::Font,
            VecLikeType::Macro => AllocKind::Macro,
            VecLikeType::ByteCode => AllocKind::ByteCode,
            VecLikeType::Timer => AllocKind::Timer,
            VecLikeType::SurfaceHandle => AllocKind::SurfaceHandle,
            VecLikeType::VideoHandle => AllocKind::VideoHandle,
        },
    };
    kind as usize
}

fn bucket(bytes: usize) -> usize {
    match bytes {
        0..=16 => 0,
        17..=32 => 1,
        33..=64 => 2,
        65..=128 => 3,
        129..=256 => 4,
        257..=512 => 5,
        513..=1024 => 6,
        1025..=4096 => 7,
        4097..=16384 => 8,
        16385..=65536 => 9,
        _ => 10,
    }
}

const BYTECODE_KIND: usize = AllocKind::ByteCode as usize;

/// Backtrace hook (call-chain evidence for probes): while armed, capture
/// a Rust backtrace for each ByteCode-kind allocation, up to the armed
/// budget. Zero cost unless a probe arms it.
static BC_TRACE_REMAINING: AtomicUsize = AtomicUsize::new(0);
static BC_TRACES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Arm the ByteCode allocation backtrace hook for the next `n`
/// ByteCode-kind allocations (clears previously captured traces).
pub(crate) fn arm_bytecode_backtraces(n: usize) {
    BC_TRACES.lock().unwrap().clear();
    BC_TRACE_REMAINING.store(n, Ordering::SeqCst);
}

/// The backtraces captured since the last `arm_bytecode_backtraces`.
pub(crate) fn bytecode_backtraces() -> Vec<String> {
    BC_TRACES.lock().unwrap().clone()
}

/// Record one non-cons allocation at link time (`link_object` /
/// `link_veclike`). The object is fully constructed before it is linked,
/// so reading its payload sizes here is sound.
pub(crate) fn record(header: *const GcHeader, addr_set_len: usize) {
    let bytes = TaggedHeap::object_bytes_from_header(header);
    let k = kind_index(header);
    COUNTS[k][bucket(bytes)].fetch_add(1, Ordering::Relaxed);
    TOTAL_BYTES[k].fetch_add(bytes as u64, Ordering::Relaxed);
    PEAK_ADDR_SET.fetch_max(addr_set_len, Ordering::Relaxed);
    if k == BYTECODE_KIND
        && BC_TRACE_REMAINING
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_sub(1))
            .is_ok()
    {
        BC_TRACES
            .lock()
            .unwrap()
            .push(std::backtrace::Backtrace::force_capture().to_string());
    }
}

/// Zero every counter (start of a probe's measured phase).
pub(crate) fn reset() {
    for row in &COUNTS {
        for cell in row {
            cell.store(0, Ordering::Relaxed);
        }
    }
    for cell in &TOTAL_BYTES {
        cell.store(0, Ordering::Relaxed);
    }
    PEAK_ADDR_SET.store(0, Ordering::Relaxed);
}

/// Peak `non_cons_object_addrs` population observed since reset.
pub(crate) fn peak_addr_set() -> usize {
    PEAK_ADDR_SET.load(Ordering::Relaxed)
}

/// The fixed (arena-resident) struct size per kind index — what a
/// size-class arena page would actually hold. Payload storage (`Vec`
/// backings, string text, hash-table internals) stays on the system
/// allocator either way.
pub(crate) fn fixed_size(kind: usize) -> usize {
    FIXED_SIZES.get(kind).copied().unwrap_or(0)
}

/// Render the per-kind allocation table: count, total bytes, fixed
/// (arena-resident) struct size, and the total-bytes histogram row.
pub(crate) fn report() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<14} {:>10} {:>13} {:>6}  {}\n",
        "kind",
        "allocs",
        "total_bytes",
        "fixed",
        BUCKET_LABELS.join(" ")
    ));
    let mut grand_allocs = 0u64;
    let mut grand_bytes = 0u64;
    for k in 0..N_KINDS {
        let count: u64 = COUNTS[k].iter().map(|c| c.load(Ordering::Relaxed)).sum();
        if count == 0 {
            continue;
        }
        let bytes = TOTAL_BYTES[k].load(Ordering::Relaxed);
        grand_allocs += count;
        grand_bytes += bytes;
        let histo: Vec<String> = COUNTS[k]
            .iter()
            .map(|c| c.load(Ordering::Relaxed).to_string())
            .collect();
        out.push_str(&format!(
            "{:<14} {:>10} {:>13} {:>6}  {}\n",
            KIND_NAMES[k],
            count,
            bytes,
            fixed_size(k),
            histo.join(" ")
        ));
    }
    out.push_str(&format!(
        "TOTAL allocs={grand_allocs} bytes={grand_bytes} peak_non_cons_object_addrs={}\n",
        peak_addr_set()
    ));
    out
}
