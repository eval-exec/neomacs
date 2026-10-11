//! The room [`raise_main_stack_rlimit`] leaves the main stack, read from
//! `/proc/self/maps` layouts taken on the machine where batch neomacs died
//! of SIGSEGV instead of signalling "Bytecode stack overflow" (T11).

use super::{main_stack_room_in, stack_guard_gap_in};

const PAGE: usize = 4096;
const GAP: usize = 256 * PAGE;
const MIB: usize = 1024 * 1024;

/// Without randomization (gdb, `setarch -R`, the agents' shells): the
/// loader's last page ends exactly 128 MiB under the stack's top, and the
/// kernel keeps the stack 1 MiB above it -- 127 MiB, not the 128 MiB the
/// limit promised.
const UNRANDOMIZED: &str = "\
7ffff7ffd000-7ffff7ffe000 r--p 00039000 00:1d 123 /nix/store/x-glibc-2.42/lib/ld-linux-x86-64.so.2
7ffff7ffe000-7ffff7fff000 rw-p 00000000 00:00 0
7ffffffb9000-7ffffffff000 rw-p 00000000 00:00 0                          [stack]
ffffffffff600000-ffffffffff601000 --xp 00000000 00:00 0                  [vsyscall]
";

#[test]
fn without_randomization_the_stack_gets_127_mib() {
    assert_eq!(main_stack_room_in(UNRANDOMIZED, GAP), Some(127 * MIB));
}

/// The same layout at the fault: the stack had grown to the gap's edge
/// (`[stack]` starting at 0x7ffff80ff000, the SIGSEGV at 0x7ffff80fefe8).
#[test]
fn the_room_is_measured_from_the_stack_top_however_far_it_has_grown() {
    let at_the_fault = UNRANDOMIZED.replace("7ffffffb9000-", "7ffff80ff000-");
    assert_eq!(main_stack_room_in(&at_the_fault, GAP), Some(127 * MIB));
    assert_eq!(0x7ffff80ff000usize, 0x7ffff7fff000 + GAP);
}

/// With randomization the mapping below is terabytes down: the 128 MiB
/// target fits and is kept.
#[test]
fn with_randomization_the_room_exceeds_the_target() {
    let randomized = "\
7715d480b000-7715d480c000 rw-p 00000000 00:00 0
7ffcadf87000-7ffcadfcd000 rw-p 00000000 00:00 0                          [stack]
";
    let room = main_stack_room_in(randomized, GAP).expect("a [stack] line");
    assert!(room > 128 * MIB, "{room:#x}");
}

#[test]
fn no_stack_line_no_answer() {
    let maps = "7ffff7ffe000-7ffff7fff000 rw-p 00000000 00:00 0\n";
    assert_eq!(main_stack_room_in(maps, GAP), None);
}

/// The kernel's gap is the `stack_guard_gap=` boot parameter in pages when
/// it is a plain number, 256 pages otherwise.
#[test]
fn the_guard_gap_follows_the_boot_parameter() {
    assert_eq!(stack_guard_gap_in("root=fstab loglevel=4", PAGE), GAP);
    assert_eq!(stack_guard_gap_in("", PAGE), GAP);
    assert_eq!(
        stack_guard_gap_in("root=fstab stack_guard_gap=1024 quiet", PAGE),
        1024 * PAGE
    );
    assert_eq!(stack_guard_gap_in("stack_guard_gap=12x", PAGE), GAP);
    assert_eq!(stack_guard_gap_in("stack_guard_gap=256", 65536), 16 * MIB);
    let bigger = main_stack_room_in(UNRANDOMIZED, 1024 * PAGE);
    assert_eq!(bigger, Some(128 * MIB - 4 * MIB));
}
