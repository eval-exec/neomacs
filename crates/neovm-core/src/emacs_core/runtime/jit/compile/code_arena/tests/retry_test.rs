//! Protection-failure recovery owns each pending run exactly once. No Lisp,
//! generated leaf, background worker or global injection state is involved.

use super::*;

static_assertions::assert_not_impl_any!(Run: Clone, Copy);

type Protection = (usize, usize);

/// Read-only test view: copying this carries no writable-range ownership.
/// Production Run deliberately has neither Clone nor Copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RunSnapshot {
    start: usize,
    bump: usize,
    end: usize,
}

impl From<&Run> for RunSnapshot {
    fn from(run: &Run) -> Self {
        Self {
            start: run.start,
            bump: run.bump,
            end: run.end,
        }
    }
}

/// Inject one failed protection by its index within this attempt, while all
/// successful operations actually change the owned pages to RX.
fn attempt(
    handle: &mut ArenaHandle,
    calls: &mut Vec<Protection>,
    fail_at: Option<usize>,
) -> io::Result<()> {
    let first = calls.len();
    handle.seal_with(|run| {
        let index = calls.len() - first;
        calls.push((run.start, page_ceil(run.bump)));
        if fail_at == Some(index) {
            Err(io::Error::from_raw_os_error(libc::EACCES))
        } else {
            protect_run(run)
        }
    })
}

fn snapshot(run: RunSnapshot) -> (usize, usize, usize) {
    (run.start, run.bump, run.end)
}

fn permissions(addr: usize) -> String {
    let maps = std::fs::read_to_string("/proc/self/maps").expect("Linux maps");
    for line in maps.lines() {
        let mut fields = line.split_whitespace();
        let range = fields.next().expect("mapping range");
        let perms = fields.next().expect("mapping permissions");
        let (lo, hi) = range.split_once('-').expect("mapping endpoints");
        let lo = usize::from_str_radix(lo, 16).expect("mapping start");
        let hi = usize::from_str_radix(hi, 16).expect("mapping end");
        if lo <= addr && addr < hi {
            return perms.to_owned();
        }
    }
    panic!("arena address {addr:#x} has a mapping")
}

fn writable(addr: usize) {
    assert!(
        permissions(addr).starts_with("rw-"),
        "{addr:#x} stays writable"
    );
}

fn executable(addr: usize) {
    assert!(
        permissions(addr).starts_with("r-x"),
        "{addr:#x} is sealed RX"
    );
}

fn write_byte(ptr: *mut u8) {
    writable(ptr as usize);
    // SAFETY: the handle just allocated this byte in its exclusively owned
    // writable run, and the permission check above precedes the store.
    unsafe { ptr.write(0xc3) };
}

/// The old bump slack plus a rounded new allocation leave one *whole* page
/// beyond the written prefix, which finalization must retain as writable.
fn open_with_tail(handle: &mut ArenaHandle) -> RunSnapshot {
    let page = page_size();
    write_byte(handle.allocate_code(1, 1).expect("first byte"));
    write_byte(handle.allocate_code(page, 1).expect("contiguous page"));
    write_byte(handle.allocate_code(page + 1, 1).expect("contiguous pages"));
    let run = RunSnapshot::from(handle.open.as_ref().expect("open run"));
    assert_eq!(run.end - page_ceil(run.bump), page);
    run
}

fn noncontiguous_with_tail(
    arena: &CodeArena,
    handle: &mut ArenaHandle,
) -> (ArenaHandle, RunSnapshot, RunSnapshot) {
    let page = page_size();
    write_byte(handle.allocate_code(1, 1).expect("first run"));
    let first = RunSnapshot::from(handle.open.as_ref().expect("first run"));
    let mut blocker = arena.handle();
    write_byte(blocker.allocate_code(page, 1).expect("intervening page"));
    write_byte(handle.allocate_code(page + 1, 1).expect("second run"));
    write_byte(handle.allocate_code(page + 2, 1).expect("grow second run"));
    let second = RunSnapshot::from(handle.open.as_ref().expect("second run"));
    assert_eq!(handle.unsealed.len(), 1);
    assert_ne!(first.end, second.start);
    assert_eq!(second.end - page_ceil(second.bump), page);
    (blocker, first, second)
}

#[test]
fn failed_open_run_retries_once_and_keeps_whole_page_capacity() {
    let arena = CodeArena::with_region_bytes(16 * page_size());
    let mut handle = arena.handle();
    let before = open_with_tail(&mut handle);
    let mut calls = Vec::new();
    let error = attempt(&mut handle, &mut calls, Some(0)).expect_err("injected protection failure");
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    assert_eq!(arena.stats().seals, 0);
    assert!(
        handle.unsealed.is_empty(),
        "open has exactly one pending owner"
    );
    assert_eq!(
        snapshot(RunSnapshot::from(
            handle.open.as_ref().expect("open retained")
        )),
        snapshot(before)
    );
    writable(before.start);
    writable(before.end - 1);

    attempt(&mut handle, &mut calls, None).expect("retry succeeds");
    let protected = (before.start, page_ceil(before.bump));
    assert_eq!(
        calls,
        [protected, protected],
        "one failed and one successful attempt"
    );
    assert_eq!(arena.stats().seals, 1, "no duplicate successful seal");
    executable(before.start);
    executable(page_ceil(before.bump) - 1);
    let tail = RunSnapshot::from(handle.open.as_ref().expect("whole writable tail retained"));
    assert_eq!(
        snapshot(tail),
        (page_ceil(before.bump), page_ceil(before.bump), before.end)
    );
    writable(tail.start);
    let next = handle.allocate_code(1, 1).expect("reuse writable tail");
    assert_eq!(next as usize, tail.start);
    write_byte(next);
    attempt(&mut handle, &mut calls, None).expect("seal later allocation");
    assert_eq!(arena.stats().seals, 2);
    assert_eq!(calls[2], (tail.start, tail.end));
    assert!(handle.open.is_none());
    executable(next as usize);
}

#[test]
fn failed_earlier_noncontiguous_run_retries_each_pending_run_once() {
    let arena = CodeArena::with_region_bytes(16 * page_size());
    let mut handle = arena.handle();
    let (_blocker, first, second) = noncontiguous_with_tail(&arena, &mut handle);
    let mut calls = Vec::new();
    attempt(&mut handle, &mut calls, Some(0)).expect_err("first run fails");
    assert_eq!(arena.stats().seals, 0);
    assert_eq!(handle.unsealed.len(), 1, "only earlier run is restored");
    assert_eq!(
        snapshot(RunSnapshot::from(&handle.unsealed[0])),
        snapshot(first)
    );
    assert_eq!(
        snapshot(RunSnapshot::from(
            handle.open.as_ref().expect("open retained")
        )),
        snapshot(second)
    );
    writable(first.start);
    writable(second.start);
    attempt(&mut handle, &mut calls, None).expect("retry both runs");
    assert_eq!(
        calls,
        [
            (first.start, page_ceil(first.bump)),
            (first.start, page_ceil(first.bump)),
            (second.start, page_ceil(second.bump)),
        ]
    );
    assert_eq!(arena.stats().seals, 2);
    assert!(handle.unsealed.is_empty());
    executable(first.start);
    executable(second.start);
    writable(page_ceil(second.bump));
}

#[test]
fn allocation_after_partial_failure_does_not_reseal_the_successful_prefix() {
    let arena = CodeArena::with_region_bytes(16 * page_size());
    let mut handle = arena.handle();
    let (_blocker, first, second) = noncontiguous_with_tail(&arena, &mut handle);
    let mut calls = Vec::new();
    attempt(&mut handle, &mut calls, Some(1)).expect_err("open fails after first run sealed");
    assert_eq!(arena.stats().seals, 1);
    assert!(handle.unsealed.is_empty());
    executable(first.start);
    writable(second.start);
    let next = handle
        .allocate_code(1, 1)
        .expect("allocate in failed writable run");
    assert_eq!(next as usize, second.bump);
    write_byte(next);
    attempt(&mut handle, &mut calls, None).expect("retry open only");
    assert_eq!(
        calls,
        [
            (first.start, page_ceil(first.bump)),
            (second.start, page_ceil(second.bump)),
            (second.start, page_ceil(second.bump)),
        ]
    );
    assert_eq!(arena.stats().seals, 2);
    executable(first.start);
    executable(next as usize);
    let tail = RunSnapshot::from(handle.open.as_ref().expect("writable tail preserved"));
    assert_eq!(tail.start, page_ceil(second.bump));
    writable(tail.start);
}

#[test]
fn displacement_after_partial_failure_moves_open_to_one_pending_owner() {
    let page = page_size();
    let arena = CodeArena::with_region_bytes(32 * page);
    let mut handle = arena.handle();
    let (mut blocker, first, second) = noncontiguous_with_tail(&arena, &mut handle);
    let mut calls = Vec::new();
    attempt(&mut handle, &mut calls, Some(1)).expect_err("open fails after first run sealed");
    executable(first.start);
    // Another module takes the next page. Our next allocation must therefore
    // move the failed open run into unsealed instead of growing it in place.
    write_byte(
        blocker
            .allocate_code(page, 1)
            .expect("new intervening page"),
    );
    let next = handle
        .allocate_code(5 * page, 1)
        .expect("new noncontiguous run");
    write_byte(next);
    let third = RunSnapshot::from(handle.open.as_ref().expect("new open run"));
    assert_eq!(third.start, next as usize);
    assert_ne!(second.end, third.start);
    assert_eq!(handle.unsealed.len(), 1, "failed run moves exactly once");
    assert_eq!(
        snapshot(RunSnapshot::from(&handle.unsealed[0])),
        snapshot(second)
    );
    writable(second.start);
    writable(third.start);
    attempt(&mut handle, &mut calls, None).expect("seal two remaining owners");
    assert_eq!(
        calls,
        [
            (first.start, page_ceil(first.bump)),
            (second.start, page_ceil(second.bump)),
            (second.start, page_ceil(second.bump)),
            (third.start, page_ceil(third.bump)),
        ]
    );
    assert_eq!(arena.stats().seals, 3);
    assert!(handle.unsealed.is_empty());
    assert!(handle.open.is_none());
    executable(first.start);
    executable(second.start);
    executable(third.start);
}
