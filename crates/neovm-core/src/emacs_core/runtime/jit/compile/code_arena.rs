//! Executable memory for the persistent JIT module (P2.4 B4 = P0.6 L4-C4).
//!
//! Cranelift's default `SystemMemoryProvider` maps a fresh anonymous region
//! for every leaf (mmap, first-touch fault, mprotect to read+execute), and
//! the region is leaked on purpose when the leaf's module drops: code is
//! never freed while the process lives, because a raw entry pointer may
//! outlive its leaf (spec slots, retired leaves). The arena keeps exactly
//! that lifetime rule — nothing it hands out is ever unmapped or written
//! again after it is sealed — and removes the per-leaf mapping:
//!
//! - [`CodeArena`] reserves large read+write regions once
//!   (`MAP_NORESERVE`: untouched pages cost nothing) and hands out whole
//!   pages by bumping a pointer; pages are pre-faulted a window at a time
//!   (`MADV_POPULATE_WRITE`) instead of one fault per leaf;
//! - an [`ArenaHandle`] (one per module generation, all of one thread's
//!   handles sharing its arena) is the module's `JITMemoryProvider`: code
//!   is bump-allocated into the pages it holds, and `finalize` seals every
//!   page written since the last seal read+execute with ONE mprotect. A
//!   sealed page is never handed out or written again, so the next leaf
//!   starts on a fresh page and code already running is never touched.
//!
//! Per synchronous compile that is one mprotect and no mmap, and the pages
//! of consecutive leaves are adjacent (one mapping, better iTLB locality).
//! Pages are not shared between leaves, so resident code memory is what it
//! was (a page per small leaf).
//!
//! x86-64 Linux only: there, instruction fetch is coherent with the data
//! writes of the same thread and cross-thread visibility of fresh code is
//! not needed (a leaf runs on the thread that compiled it). Other targets
//! keep Cranelift's provider, which does the cache maintenance they need.

use std::io;
use std::sync::{Arc, Mutex};

use cranelift_jit::{BranchProtection, JITMemoryKind, JITMemoryProvider, SystemMemoryProvider};
use cranelift_module::{ModuleError, ModuleResult};

/// Bytes reserved per region (virtual; pages are committed on use).
const REGION_BYTES: usize = 64 << 20;

/// Pages pre-faulted at a time as the bump pointer advances.
const POPULATE_BYTES: usize = 64 << 10;

fn page_size() -> usize {
    use std::sync::OnceLock;
    static PAGE: OnceLock<usize> = OnceLock::new();
    // SAFETY: sysconf has no preconditions.
    *PAGE.get_or_init(|| unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize)
}

fn page_ceil(n: usize) -> usize {
    let page = page_size();
    n.div_ceil(page) * page
}

fn align_up(n: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (n + align - 1) & !(align - 1)
}

/// Counters of one arena (the exit report's `code_memory` line).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArenaStats {
    /// Regions reserved.
    pub(crate) regions: u64,
    /// Bytes handed out to modules (whole pages).
    pub(crate) page_bytes: u64,
    /// Bytes of code actually allocated.
    pub(crate) code_bytes: u64,
    /// Seal (mprotect) calls.
    pub(crate) seals: u64,
}

struct ArenaState {
    /// The region pages are being handed out from: `[next, end)` is free.
    next: usize,
    end: usize,
    /// The pre-fault watermark in the current region.
    populated: usize,
    /// Bytes per region (a test knob shrinks it).
    region_bytes: usize,
    stats: ArenaStats,
}

impl ArenaState {
    /// `bytes` (page-aligned) of fresh read+write pages.
    fn take(&mut self, bytes: usize) -> io::Result<(usize, usize)> {
        debug_assert_eq!(bytes % page_size(), 0);
        if self.end - self.next < bytes {
            self.reserve(bytes)?;
        }
        let start = self.next;
        self.next += bytes;
        self.stats.page_bytes += bytes as u64;
        if self.next > self.populated {
            self.prefault();
        }
        Ok((start, start + bytes))
    }

    /// Reserve a new region able to hold `bytes`. The rest of the old one is
    /// abandoned (address space only: its pages were never touched).
    fn reserve(&mut self, bytes: usize) -> io::Result<()> {
        let len = page_ceil(self.region_bytes.max(bytes));
        // SAFETY: a fresh private anonymous mapping; no existing memory is
        // affected. Never unmapped: code handed out from it lives as long as
        // the process.
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let base = base as usize;
        self.next = base;
        self.end = base + len;
        self.populated = base;
        self.stats.regions += 1;
        Ok(())
    }

    /// Fault in the next window of the region in one call instead of one
    /// fault per new page. Best effort: an old kernel refuses the advice
    /// and the pages fault in on first write as before.
    fn prefault(&mut self) {
        let from = self.populated;
        let to = align_up(self.next, POPULATE_BYTES).min(self.end);
        if to > from {
            // SAFETY: [from, to) lies inside a live read+write mapping of
            // this arena; populating only allocates its zero pages.
            unsafe {
                libc::madvise(
                    from as *mut libc::c_void,
                    to - from,
                    libc::MADV_POPULATE_WRITE,
                );
            }
        }
        self.populated = to;
    }
}

/// One thread's code arena, shared by the handles of every module
/// generation it creates. Cheap to clone (an `Arc`).
#[derive(Clone)]
pub(crate) struct CodeArena {
    state: Arc<Mutex<ArenaState>>,
}

impl CodeArena {
    pub(crate) fn new() -> CodeArena {
        CodeArena::with_region_bytes(REGION_BYTES)
    }

    /// An arena whose regions are `region_bytes` long (tests exercise the
    /// region rollover with a few pages).
    pub(crate) fn with_region_bytes(region_bytes: usize) -> CodeArena {
        CodeArena {
            state: Arc::new(Mutex::new(ArenaState {
                next: 0,
                end: 0,
                populated: 0,
                region_bytes: page_ceil(region_bytes.max(1)),
                stats: ArenaStats::default(),
            })),
        }
    }

    /// A memory provider for one module, drawing pages from this arena.
    pub(crate) fn handle(&self) -> ArenaHandle {
        ArenaHandle {
            arena: self.clone(),
            open: None,
            unsealed: Vec::new(),
            other: SystemMemoryProvider::new(),
        }
    }

    pub(crate) fn stats(&self) -> ArenaStats {
        self.lock().stats
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ArenaState> {
        // Poison-tolerant: the state is plain integers, consistent between
        // statements.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// A run of pages a handle holds: `[start, bump)` written, `[bump, end)`
/// free, nothing sealed.
#[derive(Clone, Copy, Debug)]
struct Run {
    start: usize,
    bump: usize,
    end: usize,
}

/// The `JITMemoryProvider` of one module. See the module docs.
pub(crate) struct ArenaHandle {
    arena: CodeArena,
    /// The run new code goes into.
    open: Option<Run>,
    /// Earlier runs written since the last seal (not contiguous with
    /// `open`).
    unsealed: Vec<Run>,
    /// Non-code kinds (data objects; the JIT defines none).
    other: SystemMemoryProvider,
}

impl ArenaHandle {
    fn allocate_code(&mut self, size: usize, align: usize) -> io::Result<*mut u8> {
        if align > page_size() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "code alignment above the page size",
            ));
        }
        if let Some(run) = &mut self.open {
            let at = align_up(run.bump, align);
            if at + size <= run.end {
                run.bump = at + size;
                self.arena.lock().stats.code_bytes += size as u64;
                return Ok(at as *mut u8);
            }
        }
        let mut arena = self.arena.lock();
        let (start, end) = arena.take(page_ceil(size.max(1)))?;
        arena.stats.code_bytes += size as u64;
        drop(arena);
        match &mut self.open {
            // Contiguous with the open run: grow it (one seal later).
            Some(run) if run.end == start => {
                let at = align_up(run.bump, align);
                debug_assert!(at + size <= end);
                run.end = end;
                run.bump = at + size;
                Ok(at as *mut u8)
            }
            open => {
                if let Some(run) = open.take()
                    && run.bump > run.start
                {
                    self.unsealed.push(run);
                }
                *open = Some(Run {
                    start,
                    bump: start + size,
                    end,
                });
                Ok(start as *mut u8)
            }
        }
    }

    /// Make every page written since the last seal read+execute. The open
    /// run's unwritten whole pages stay writable for the next leaf.
    fn seal(&mut self) -> io::Result<()> {
        let mut runs = std::mem::take(&mut self.unsealed);
        if let Some(run) = self.open
            && run.bump > run.start
        {
            runs.push(run);
        }
        for (i, run) in runs.iter().enumerate() {
            let sealed_end = page_ceil(run.bump);
            // SAFETY: `[start, sealed_end)` is page-aligned, inside this
            // arena's live mapping, and holds only code this handle wrote
            // and nobody executes yet; dropping write access is the seal.
            let rc = unsafe {
                libc::mprotect(
                    run.start as *mut libc::c_void,
                    sealed_end - run.start,
                    libc::PROT_READ | libc::PROT_EXEC,
                )
            };
            if rc != 0 {
                let err = io::Error::last_os_error();
                // Keep what is not sealed yet for the next attempt.
                self.unsealed.extend_from_slice(&runs[i..]);
                return Err(err);
            }
            self.arena.lock().stats.seals += 1;
        }
        if let Some(run) = &mut self.open
            && run.bump > run.start
        {
            let sealed_end = page_ceil(run.bump);
            if sealed_end >= run.end {
                self.open = None;
            } else {
                run.start = sealed_end;
                run.bump = sealed_end;
            }
        }
        Ok(())
    }
}

impl JITMemoryProvider for ArenaHandle {
    fn allocate(&mut self, size: usize, align: u64, kind: JITMemoryKind) -> io::Result<*mut u8> {
        match kind {
            JITMemoryKind::Executable => {
                let align = usize::try_from(align).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "code alignment too large")
                })?;
                self.allocate_code(size, align.max(1))
            }
            JITMemoryKind::Writable | JITMemoryKind::ReadOnly => {
                self.other.allocate(size, align, kind)
            }
        }
    }

    unsafe fn free_memory(&mut self) {
        // Code pages are never returned (see the module docs); only the
        // data provider has anything of its own to free.
        // SAFETY: forwarded under the caller's contract.
        unsafe { self.other.free_memory() };
    }

    fn finalize(&mut self, branch_protection: BranchProtection) -> ModuleResult<()> {
        // The sealing mprotect is the arena's only fallible finalize step;
        // it fails like an allocation (address-space or mapping limits).
        self.seal().map_err(|err| ModuleError::Allocation { err })?;
        self.other.finalize(branch_protection)
    }
}
