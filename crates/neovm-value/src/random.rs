//! Process PRNG substrate (GNU `sysdep.c`).
//!
//! The srandom/random serialization, the fixnum-width word mixer, and the
//! seeding entry points mirror GNU's `emacs_get_random` family
//! (`src/sysdep.c`; `get_random_ulong` at 2270-2276). The `random` subr and
//! the bignum sampler stay in the upper arithmetic layer and reach the
//! substrate through `with_random_locked` / `emacs_random_limb_unlocked`, so
//! the single process lock keeps its one owner. Moved from
//! `emacs_core/lisp/native/builtins/arithmetic.rs` with the stage-3 split
//! (C12): the buffer substrate generates unique names with it, below the
//! lisp layer.

use std::sync::{Mutex, MutexGuard};

/// Whether the process `random' generator has been given a seed.
///
/// GNU's generator is libc's `random ()` state, and `main` seeds it on every
/// startup, dumped image or not, before any Lisp runs (`init_random ()` at
/// src/emacs.c:2020, below `load_pdump` and outside the `!initialized`
/// block). A generator nobody seeded is not random: POSIX has it answer the
/// `srandom (1)` sequence, so every process drew the same numbers and
/// `server-generate-key' (lisp/server.el:606-617) computed the same TCP
/// server key in every session. Neither libc's state nor this one is part
/// of a dumped image, so every process starts `Unseeded`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratorSeed {
    Unseeded,
    Seeded,
}

fn lock_generator() -> MutexGuard<'static, GeneratorSeed> {
    static GENERATOR: Mutex<GeneratorSeed> = Mutex::new(GeneratorSeed::Unseeded);
    GENERATOR
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// GNU `init_random` (src/sysdep.c:2225-2249): seed from the operating
/// system's entropy source (`getrandom`, or `w32_init_random` on Windows),
/// and from the PID and the current time only when that source fails.
fn seed_from_entropy(seed: &mut MutexGuard<'static, GeneratorSeed>) {
    let value = getrandom::u32().unwrap_or_else(|_| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        std::process::id() ^ (now.as_secs() as u32) ^ now.subsec_nanos()
    });
    platform_seed_random(value);
    **seed = GeneratorSeed::Seeded;
}

fn emacs_intmask() -> u64 {
    (1_u64 << emacs_random_fixnum_bits()) - 1
}

fn emacs_random_fixnum_bits() -> u32 {
    62
}

fn emacs_get_random_unlocked() -> i64 {
    const RAND_BITS: u32 = 31;
    const EMACS_INT_WIDTH: u32 = 64;
    let fixnum_bits = emacs_random_fixnum_bits();
    let mut val: u64 = 0;
    for _ in 0..fixnum_bits.div_ceil(RAND_BITS) {
        let r = platform_random_word();
        val = r ^ (val << RAND_BITS) ^ (val >> (EMACS_INT_WIDTH - RAND_BITS));
    }
    val ^= val >> (EMACS_INT_WIDTH - fixnum_bits);
    (val & emacs_intmask()) as i64
}

/// Run `body` while holding the process PRNG lock. The bignum sampler
/// rejects whole candidates against its limit, so the lock must span every
/// limb of one sample, exactly as GNU's `random` loop in `src/bignum.c`
/// does; keeping the guard inside the substrate leaves the lock with a
/// single owner.
#[doc(hidden)]
pub fn with_random_locked<R>(body: impl FnOnce() -> R) -> R {
    let mut state = lock_generator();
    if *state == GeneratorSeed::Unseeded {
        seed_from_entropy(&mut state);
    }
    // Hold the generator lock for the whole body: one bignum sample spans
    // several limbs, exactly as GNU's `random` loop in src/bignum.c does.
    let _held = state;
    body()
}

/// A full-width `emacs_get_random()` fixnum value, as GNU's unbounded
/// `random` returns.
#[doc(hidden)]
pub fn emacs_get_random() -> i64 {
    with_random_locked(emacs_get_random_unlocked)
}

/// A uniform fixnum in `[0, limit)`, matching GNU's bounded `random`
/// rejection loop.
#[doc(hidden)]
pub fn emacs_get_random_fixnum(limit: i64) -> i64 {
    let lim = limit as u64;
    let intmask = emacs_intmask();
    let difflim = intmask - lim + 1;
    with_random_locked(|| {
        loop {
            let r = emacs_get_random_unlocked() as u64;
            let remainder = r % lim;
            let diff = r - remainder;
            if difflim >= diff {
                return remainder as i64;
            }
        }
    })
}

#[doc(hidden)]
pub fn emacs_seed_random(seed: &[u8]) {
    let mut arg = 0u32;
    for (index, byte) in seed.iter().enumerate() {
        arg ^= u32::from(*byte) << ((index % 4) * 8);
    }
    let mut state = lock_generator();
    platform_seed_random(arg);
    // The seed is deterministic and it is the process's seed from now on,
    // so no later draw replaces it with one from the OS.
    *state = GeneratorSeed::Seeded;
}

#[doc(hidden)]
pub fn emacs_init_random() {
    seed_from_entropy(&mut lock_generator());
}

/// One PRNG word mixed into a full `u64` limb, for the bignum sampler.
/// Callers must already hold the process lock (`with_random_locked`);
/// GNU `src/sysdep.c:2270-2276`, `get_random_ulong`.
#[doc(hidden)]
pub fn emacs_random_limb_unlocked() -> u64 {
    let mut word = 0u64;
    for _ in 0..64u32.div_ceil(31) {
        word = platform_random_word() ^ (word << 31) ^ (word >> 33);
    }
    word
}

#[cfg(unix)]
fn platform_seed_random(seed: u32) {
    // GNU sysdep.c uses srandom/random when HAVE_RANDOM is available, with
    // `unsigned int random_seed`.  Unix platforms we support provide that API.
    unsafe { c_random::srandom(seed as libc::c_uint) };
}

#[cfg(unix)]
fn platform_random_word() -> u64 {
    unsafe { c_random::random() as u64 }
}

#[cfg(unix)]
mod c_random {
    unsafe extern "C" {
        pub(super) fn srandom(seed: libc::c_uint);
        pub(super) fn random() -> libc::c_long;
    }
}

#[cfg(not(unix))]
mod fallback_random {
    use std::sync::atomic::{AtomicU32, Ordering};

    /// One state for the whole process, not one per thread: the seed set by
    /// startup or by `(random STRING)` has to govern every Lisp thread's
    /// draws. GNU replaces the MS-Windows `rand` for the same reason, "MS-
    /// Windows 'rand' produces separate identical series for each thread"
    /// (src/w32.c:2419-2457). Only read and written under the generator
    /// lock (`lock_generator`).
    static RANDOM_STATE: AtomicU32 = AtomicU32::new(0x1234_5678);

    pub(super) fn seed(seed: u32) {
        RANDOM_STATE.store(seed, Ordering::Relaxed);
    }

    pub(super) fn next() -> u64 {
        let next = RANDOM_STATE
            .load(Ordering::Relaxed)
            .wrapping_mul(1103515245)
            .wrapping_add(12345);
        RANDOM_STATE.store(next, Ordering::Relaxed);
        u64::from((next >> 16) & 0x7fff)
    }
}

#[cfg(not(unix))]
fn platform_seed_random(seed: u32) {
    fallback_random::seed(seed);
}

#[cfg(not(unix))]
fn platform_random_word() -> u64 {
    fallback_random::next()
}
