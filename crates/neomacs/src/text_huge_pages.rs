//! Fold this process's own executable text into 2 MiB file-backed huge pages
//! via MADV_COLLAPSE. GNU Emacs has no counterpart: this is a Neo-only win
//! measured in tmp/t12/DESIGN.md. Failure is silent by design: any refusal
//! leaves the process exactly as today. There are deliberately no env knobs.

#[cfg(target_env = "gnu")]
use libc::MADV_COLLAPSE;

// Linux's MADV_COLLAPSE value; non-GNU libc bindings do not expose it.
#[cfg(not(target_env = "gnu"))]
const MADV_COLLAPSE: libc::c_int = 25;

const CHUNK_SIZE: usize = 2 * 1024 * 1024;

// ELF's PF_X segment execute flag; libc's PF_* constants are socket families.
const ELF_PF_X: u32 = 1;

pub(crate) fn start_background_collapse() {
    // Override the workspace's 128 MiB RUST_MIN_STACK for this small worker.
    let _ = std::thread::Builder::new()
        .name("text-huge-pages".into())
        .stack_size(128 * 1024)
        .spawn(collapse_executable_text_chunks);
}

struct ChunkRange {
    start: usize,
    len: usize,
}

impl ChunkRange {
    fn new(start: usize) -> Self {
        debug_assert!(start.is_multiple_of(CHUNK_SIZE));
        Self {
            start,
            len: CHUNK_SIZE,
        }
    }

    fn start(&self) -> usize {
        self.start
    }

    fn end(&self) -> usize {
        self.start.saturating_add(self.len)
    }

    fn len(&self) -> usize {
        self.len
    }
}

fn inward_chunks(seg_start: usize, seg_end: usize) -> Vec<ChunkRange> {
    if seg_start >= seg_end {
        return Vec::new();
    }
    let remainder = seg_start % CHUNK_SIZE;
    let adjustment = if remainder == 0 {
        0
    } else {
        CHUNK_SIZE - remainder
    };
    let Some(mut start) = seg_start.checked_add(adjustment) else {
        return Vec::new();
    };
    let end = seg_end - seg_end % CHUNK_SIZE;
    let mut chunks = Vec::new();
    while let Some(chunk_end) = start.checked_add(CHUNK_SIZE) {
        if chunk_end > end {
            break;
        }
        let chunk = ChunkRange::new(start);
        start = chunk.end();
        chunks.push(chunk);
    }
    chunks
}

#[derive(Debug, PartialEq, Eq)]
enum ThpPolicy {
    Always,
    Madvise,
    Never,
    Unknown,
}

fn parse_thp_enabled(contents: &str) -> ThpPolicy {
    let mut selected = None;
    for word in contents.split_whitespace() {
        if word.contains(['[', ']']) {
            if selected.is_some() {
                return ThpPolicy::Unknown;
            }
            selected = Some(match word {
                "[always]" => ThpPolicy::Always,
                "[madvise]" => ThpPolicy::Madvise,
                "[never]" => ThpPolicy::Never,
                _ => return ThpPolicy::Unknown,
            });
        } else if !matches!(word, "always" | "madvise" | "never") {
            return ThpPolicy::Unknown;
        }
    }
    match selected {
        Some(policy) => policy,
        None => ThpPolicy::Unknown,
    }
}

fn current_thp_policy() -> ThpPolicy {
    match std::fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled") {
        Ok(contents) => parse_thp_enabled(&contents),
        Err(_) => ThpPolicy::Unknown,
    }
}

enum ChunkOutcome {
    Collapsed,
    Refused,
}

fn collapse_chunk(chunk: ChunkRange) -> ChunkOutcome {
    // SAFETY: The loader supplies a mapped, executable file range, and
    // inward_chunks restricts this request to one whole aligned chunk inside
    // that range. MADV_COLLAPSE preserves its contents and permissions.
    let result = unsafe {
        libc::madvise(
            chunk.start() as *mut libc::c_void,
            chunk.len(),
            MADV_COLLAPSE,
        )
    };
    if result == 0 {
        ChunkOutcome::Collapsed
    } else {
        ChunkOutcome::Refused
    }
}

unsafe extern "C" fn phdr_callback(
    info: *mut libc::dl_phdr_info,
    _size: usize,
    data: *mut libc::c_void,
) -> libc::c_int {
    // SAFETY: dl_iterate_phdr supplies a valid info pointer for this callback;
    // the loader keeps the object and its program headers alive during it.
    let Some(info) = (unsafe { info.as_ref() }) else {
        return 0;
    };
    if !info.dlpi_name.is_null() {
        // SAFETY: The loader supplies a NUL-terminated object name. Reading
        // its first byte is valid even for the main executable's empty name.
        if unsafe { *info.dlpi_name } != 0 {
            return 0;
        }
    }
    if info.dlpi_phdr.is_null() {
        return 0;
    }
    // SAFETY: data points to the calling thread's live totals, used only by
    // these synchronous callbacks, so this mutable borrow is exclusive.
    let Some(totals) = (unsafe { data.cast::<(usize, usize)>().as_mut() }) else {
        return 0;
    };
    // SAFETY: The loader provides dlpi_phnum initialized headers at dlpi_phdr
    // and holds their mapping valid throughout this callback.
    let headers =
        unsafe { std::slice::from_raw_parts(info.dlpi_phdr, usize::from(info.dlpi_phnum)) };
    for header in headers {
        if header.p_type != libc::PT_LOAD
            || header.p_flags & ELF_PF_X == 0
            || u64::from(header.p_align) < CHUNK_SIZE as u64
        {
            continue;
        }
        let (Ok(base), Ok(vaddr), Ok(filesz)) = (
            usize::try_from(info.dlpi_addr),
            usize::try_from(header.p_vaddr),
            usize::try_from(header.p_filesz),
        ) else {
            continue;
        };
        let Some(seg_start) = base.checked_add(vaddr) else {
            continue;
        };
        let Some(seg_end) = seg_start.checked_add(filesz) else {
            continue;
        };
        for chunk in inward_chunks(seg_start, seg_end) {
            match collapse_chunk(chunk) {
                ChunkOutcome::Collapsed => totals.0 = totals.0.saturating_add(1),
                ChunkOutcome::Refused => totals.1 = totals.1.saturating_add(1),
            }
        }
    }
    0
}

fn collapse_executable_text_chunks() {
    if current_thp_policy() == ThpPolicy::Never {
        return;
    }
    // Unknown proceeds: sysfs may be absent in sandboxes; refusals are harmless.
    let mut totals = (0usize, 0usize);
    // SAFETY: dl_iterate_phdr calls back synchronously on this thread and
    // retains neither callback nor data, so totals outlives every use. The
    // dynamic loader lock keeps the phdr pointers valid during the walk.
    unsafe {
        libc::dl_iterate_phdr(
            Some(phdr_callback),
            &mut totals as *mut _ as *mut libc::c_void,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inward_chunks_exactly_aligned() {
        let chunks = inward_chunks(CHUNK_SIZE, 2 * CHUNK_SIZE);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks.first().map(ChunkRange::start), Some(CHUNK_SIZE));
        assert_eq!(chunks.first().map(ChunkRange::end), Some(2 * CHUNK_SIZE));
    }

    #[test]
    fn inward_chunks_four_mib() {
        let starts: Vec<_> = inward_chunks(0, 2 * CHUNK_SIZE)
            .iter()
            .map(ChunkRange::start)
            .collect();
        assert_eq!(starts, vec![0, CHUNK_SIZE]);
    }

    #[test]
    fn inward_chunks_rounds_both_boundaries_inward() {
        let starts: Vec<_> = inward_chunks(0x101000, 0x601000)
            .iter()
            .map(ChunkRange::start)
            .collect();
        assert_eq!(starts, vec![0x200000, 0x400000]);

        // The chunk at 0x400000 ends at 0x600000, beyond this segment.
        let shorter_starts: Vec<_> = inward_chunks(0x101000, 0x501000)
            .iter()
            .map(ChunkRange::start)
            .collect();
        assert_eq!(shorter_starts, vec![0x200000]);
    }

    #[test]
    fn inward_chunks_empty_ranges() {
        for (start, end) in [
            (0, CHUNK_SIZE - 1),
            (0x1000, 0x1ff000),
            (CHUNK_SIZE, CHUNK_SIZE),
            (2 * CHUNK_SIZE, CHUNK_SIZE),
        ] {
            assert!(inward_chunks(start, end).is_empty());
        }
    }

    #[test]
    fn inward_chunks_overflow_is_empty() {
        assert!(inward_chunks(usize::MAX - 1, usize::MAX).is_empty());
        assert!(inward_chunks(usize::MAX - (CHUNK_SIZE - 1), usize::MAX).is_empty());
    }

    #[test]
    fn inward_chunks_invariants() {
        for chunk in inward_chunks(0x101000, 0xa01000) {
            assert_eq!(chunk.start() % CHUNK_SIZE, 0);
            assert_eq!(chunk.len(), CHUNK_SIZE);
            assert_eq!(chunk.end() - chunk.start(), CHUNK_SIZE);
            assert!(chunk.start() >= 0x101000);
            assert!(chunk.end() <= 0xa01000);
        }
    }

    #[test]
    fn parse_thp_enabled_policies() {
        assert_eq!(
            parse_thp_enabled("always [madvise] never"),
            ThpPolicy::Madvise
        );
        assert_eq!(
            parse_thp_enabled("[always] madvise never"),
            ThpPolicy::Always
        );
        assert_eq!(
            parse_thp_enabled("always madvise [never]"),
            ThpPolicy::Never
        );
    }

    #[test]
    fn parse_thp_enabled_unknown() {
        for contents in [
            "",
            "garbage",
            "always madvise never",
            "[garbage]",
            "[always] [never]",
            "[always",
            "always]",
            "[never] garbage",
        ] {
            assert_eq!(parse_thp_enabled(contents), ThpPolicy::Unknown);
        }
    }

    #[test]
    fn collapse_executable_text_chunks_smoke() {
        collapse_executable_text_chunks();
    }
}
