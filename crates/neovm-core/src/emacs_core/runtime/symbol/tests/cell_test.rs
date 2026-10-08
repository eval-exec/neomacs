//! The value cell's private representation: the flags byte, the typed
//! arms, the single write seam and the GC's seqlock reader.

use super::*;
use crate::emacs_core::forward::FwdDescriptor;
use crate::emacs_core::intern::intern;
use crate::emacs_core::symbol::NO_WHERE_BUF;
use std::sync::atomic::{AtomicBool, AtomicU32};

/// A cell write off the concurrent mark: what every test here needs, since
/// no mark runs in a unit test.
fn idle_write<'a>(sym: &'a mut LispSymbol, seq: &'a AtomicU32) -> CellWrite<'a> {
    let gate = MarkGate::read();
    assert!(
        !gate.is_marking(),
        "unit tests run outside a concurrent mark"
    );
    CellWrite::begin(sym, || seq, gate)
}

/// SymbolFlags packs into a single byte (matches GNU's bit layout).
#[test]
fn symbol_flags_pack_into_one_byte() {
    crate::test_utils::init_test_tracing();
    assert_eq!(std::mem::size_of::<SymbolFlags>(), 1);
}

/// Bit 7 (this port's `runtime_projected`) round-trips without disturbing
/// GNU's four fields, and the one-byte plain-untrapped test is true for
/// exactly the (Plainval, Untrapped, unprojected) shape.
#[test]
fn runtime_projected_bit_is_independent_of_gnu_symbol_fields() {
    crate::test_utils::init_test_tracing();
    let mut flags = SymbolFlags::default();
    flags.set_redirect(SymbolRedirect::Localized);
    flags.set_trapped_write(SymbolTrappedWrite::Trapped);
    flags.set_interned(SymbolInterned::InternedInInitial);
    flags.set_declared_special(true);
    flags.set_runtime_projected(true);
    assert_eq!(flags.redirect(), SymbolRedirect::Localized);
    assert_eq!(flags.trapped_write(), SymbolTrappedWrite::Trapped);
    assert_eq!(flags.interned(), SymbolInterned::InternedInInitial);
    assert!(flags.declared_special());
    assert!(flags.runtime_projected());
    flags.set_runtime_projected(false);
    assert!(!flags.runtime_projected());
    assert_eq!(flags.redirect(), SymbolRedirect::Localized);
    assert!(flags.declared_special());

    for redirect in [
        SymbolRedirect::Plainval,
        SymbolRedirect::Varalias,
        SymbolRedirect::Localized,
        SymbolRedirect::Forwarded,
    ] {
        for trapped in [
            SymbolTrappedWrite::Untrapped,
            SymbolTrappedWrite::NoWrite,
            SymbolTrappedWrite::Trapped,
        ] {
            for projected in [false, true] {
                let mut f = SymbolFlags::default();
                f.set_redirect(redirect);
                f.set_trapped_write(trapped);
                f.set_runtime_projected(projected);
                f.set_declared_special(true);
                let want = redirect == SymbolRedirect::Plainval
                    && trapped == SymbolTrappedWrite::Untrapped
                    && !projected;
                assert_eq!(
                    f.is_plain_untrapped_unprojected(),
                    want,
                    "{redirect:?} {trapped:?} projected={projected}"
                );
            }
        }
    }
}

/// A fresh symbol is GNU's `init_symbol` state: Plainval / UNBOUND, and the
/// typed readers agree with the tag.
#[test]
fn fresh_symbol_reads_as_an_unbound_plain_cell() {
    let sym = LispSymbol::new(intern("cell-fresh"));
    assert!(matches!(sym.value_cell(), ValueCell::Plain(v) if v.is_unbound()));
    assert_eq!(
        sym.plain_value().map(Value::bits),
        Some(Value::UNBOUND.bits())
    );
    assert_eq!(sym.alias_target(), None);
    assert!(sym.localized_blv().is_none());
    assert!(sym.forwarded_descriptor().is_none());
}

/// Every typed reader answers only for its own arm: the tag decides, so no
/// word is ever read as another arm's payload.
#[test]
fn each_reader_answers_only_for_its_own_arm() {
    let seq = AtomicU32::new(0);
    let target = intern("cell-alias-target");
    let fwd = crate::emacs_core::forward::alloc_objfwd(Value::fixnum(5)).header();
    let blv = BlvPtr::leak(Box::new(LispBufferLocalValue {
        local_if_set: false,
        found: false,
        fwd: None,
        where_buf: Value::NIL,
        where_buf_id: NO_WHERE_BUF,
        defcell: Value::NIL,
        valcell: Value::NIL,
        alist_epoch: 0,
    }));

    let mut aliased = LispSymbol::new(intern("cell-aliased"));
    match idle_write(&mut aliased, &seq).arm() {
        ArmMut::Plain(plain) => plain.alias_to(target),
        _ => unreachable!("fresh cells are plain"),
    }
    assert_eq!(aliased.redirect(), SymbolRedirect::Varalias);
    assert_eq!(aliased.alias_target(), Some(target));
    assert_eq!(aliased.plain_value().map(Value::bits), None);
    assert!(aliased.forwarded_descriptor().is_none());
    assert!(aliased.localized_blv().is_none());

    let mut forwarded = LispSymbol::new(intern("cell-forwarded"));
    match idle_write(&mut forwarded, &seq).arm() {
        ArmMut::Plain(plain) => plain.forward_to(fwd),
        _ => unreachable!("fresh cells are plain"),
    }
    assert!(std::ptr::eq(
        forwarded.forwarded_descriptor().expect("forwarded"),
        fwd
    ));
    assert_eq!(forwarded.plain_value().map(Value::bits), None);
    assert_eq!(forwarded.alias_target(), None);

    let mut localized = LispSymbol::new(intern("cell-localized"));
    match idle_write(&mut localized, &seq).arm() {
        ArmMut::Plain(plain) => plain.localize(blv),
        _ => unreachable!("fresh cells are plain"),
    }
    assert_eq!(localized.localized_blv(), Some(blv));
    assert_eq!(localized.plain_value().map(Value::bits), None);
    assert!(localized.forwarded_descriptor().is_none());
    assert_eq!(
        localized.value_cell_acquire().redirect_for_test(),
        SymbolRedirect::Localized
    );

    // SAFETY: the record was leaked above for this test only, and no cell
    // that names it is read again.
    unsafe { blv.free() };
}

/// A localized cell offers no transition but its deep-copy re-home: the
/// value of a buffer-local variable lives in its record, never in the cell,
/// so GNU never moves it to another arm.
#[test]
fn a_localized_cell_keeps_its_arm() {
    let seq = AtomicU32::new(0);
    let make = || {
        BlvPtr::leak(Box::new(LispBufferLocalValue {
            local_if_set: true,
            found: false,
            fwd: None,
            where_buf: Value::NIL,
            where_buf_id: NO_WHERE_BUF,
            defcell: Value::NIL,
            valcell: Value::NIL,
            alist_epoch: 0,
        }))
    };
    let (first, second) = (make(), make());
    let mut sym = LispSymbol::new(intern("cell-localized-keeps"));
    match idle_write(&mut sym, &seq).arm() {
        ArmMut::Plain(plain) => plain.localize(first),
        _ => unreachable!(),
    }
    match idle_write(&mut sym, &seq).arm() {
        ArmMut::Localized(local) => {
            assert_eq!(local.blv(), first);
            local.rehome(second);
        }
        ArmMut::Plain(_) | ArmMut::Alias(_) | ArmMut::Forwarded(_) => {
            panic!("a localized cell must stay localized")
        }
    }
    assert_eq!(sym.localized_blv(), Some(second));
    // SAFETY: test-owned records; no cell is read after this.
    unsafe {
        first.free();
        second.free();
    }
}

/// An alias is undone into a plain cell with the value it is given -- GNU
/// `Fmakunbound` / `internal-delete-indirect-variable` on an alias.
#[test]
fn an_alias_unaliases_into_a_plain_cell() {
    let seq = AtomicU32::new(0);
    let mut sym = LispSymbol::new(intern("cell-unalias"));
    match idle_write(&mut sym, &seq).arm() {
        ArmMut::Plain(plain) => plain.alias_to(intern("cell-unalias-base")),
        _ => unreachable!(),
    }
    match idle_write(&mut sym, &seq).arm() {
        ArmMut::Alias(alias) => alias.unalias(Value::UNBOUND),
        _ => unreachable!(),
    }
    assert!(matches!(sym.value_cell(), ValueCell::Plain(v) if v.is_unbound()));
}

/// The seqlock is held odd for exactly the life of a write made during a
/// mark, and never touched off it.
#[test]
fn a_cell_write_brackets_the_seqlock_only_while_marking() {
    let seq = AtomicU32::new(0);
    let mut sym = LispSymbol::new(intern("cell-seq"));
    {
        let mut write = idle_write(&mut sym, &seq);
        if let ArmMut::Plain(plain) = write.arm() {
            plain.store(Value::fixnum(1));
        }
        assert_eq!(seq.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
    assert_eq!(seq.load(std::sync::atomic::Ordering::Relaxed), 0);
    {
        let write = CellWrite::begin(&mut sym, || &seq, MarkGate { marking: true });
        assert_eq!(
            seq.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "odd while writing"
        );
        drop(write);
    }
    assert_eq!(
        seq.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "even after"
    );
}

/// The dump constructor takes the tag and its word as one value.
#[test]
fn a_dumped_cell_restores_its_tag_with_its_word() {
    let flags = DumpedSymbolFlags {
        trapped_write: SymbolTrappedWrite::Trapped,
        interned: SymbolInterned::Interned,
        declared_special: true,
    };
    let target = intern("cell-dump-target");
    let alias = LispSymbol::from_dump(
        intern("cell-dump-alias"),
        flags,
        DumpedCell::Alias(target),
        Value::NIL,
        Value::NIL,
    );
    assert_eq!(alias.redirect(), SymbolRedirect::Varalias);
    assert_eq!(alias.alias_target(), Some(target));
    assert_eq!(alias.trapped_write(), SymbolTrappedWrite::Trapped);
    assert_eq!(alias.flags().interned(), SymbolInterned::Interned);
    assert!(alias.flags().declared_special());

    let plain = LispSymbol::from_dump(
        intern("cell-dump-plain"),
        flags,
        DumpedCell::Plain(Value::fixnum(9)),
        Value::NIL,
        Value::NIL,
    );
    assert_eq!(
        plain.plain_value().map(Value::bits),
        Some(Value::fixnum(9).bits())
    );
}

impl ValueCell {
    fn redirect_for_test(self) -> SymbolRedirect {
        match self {
            ValueCell::Plain(_) => SymbolRedirect::Plainval,
            ValueCell::Alias(_) => SymbolRedirect::Varalias,
            ValueCell::Localized(_) => SymbolRedirect::Localized,
            ValueCell::Forwarded(_) => SymbolRedirect::Forwarded,
        }
    }
}

// ===========================================================================
// Stage 1b seqlock symbol-read protocol: torn-arm-read defense
// ===========================================================================
//
// These two tests prove that `read_symbol_children_consistent` (the GC-thread
// read side of the per-chunk seqlock) never returns a value read from the WRONG
// arm under a concurrent writer that flips a symbol between two redirect
// states. The positive test asserts zero torn reads under the real protocol; the
// negative control holds the writer inside the torn window and proves that the
// same read without the seqlock retry accepts an inconsistent arm/value pair.

/// Two distinct "heap-looking" `Value`s minted from raw tagged bits.
///
/// `TAG_CONS == 0b011`. A word `(fake_ptr | TAG_CONS)` with `fake_ptr`
/// 8-aligned has `tag() == TAG_CONS`, so `Value::is_heap_object()` returns
/// `true` (it matches `TAG_CONS | TAG_STRING | TAG_FLOAT | TAG_VECLIKE`).
/// The pointer is NEVER dereferenced by the test — only its bits are compared
/// and its heap-object-ness exercised — so a fake address is sound here. The
/// two values differ in the high bits, so a torn read that swaps one for the
/// other is detectable by value comparison.
fn heap_a() -> Value {
    // 0x1_0000 | 0b011 = 0x1_0003. 8-aligned base, cons tag.
    Value::from_bits(0x1_0000 | crate::tagged::value::TAG_CONS)
}
fn heap_b() -> Value {
    // 0x2_0000 | 0b011 = 0x2_0003. Distinct 8-aligned base, cons tag.
    Value::from_bits(0x2_0000 | crate::tagged::value::TAG_CONS)
}

/// Raw-pointer bundle to share the symbol + seqlock across threads. The only
/// cross-thread accesses are the atomic word/flag stores on the writer side and
/// the seqlock-protocol atomic loads on the reader side, mirroring the
/// production `ConsCell` / per-chunk-seqlock pattern (single mutator, single GC
/// reader). Hence `Send` is sound.
struct Shared(*mut LispSymbol, *const AtomicU32);
unsafe impl Send for Shared {}

/// Number of writer arm-flips in the protected concurrent stress test.
const SEQLOCK_WRITER_ITERS: u64 = 4_000_000;

/// Store WORD into SYM's cell the way `CellWrite::publish` does, but under a
/// tag of the test's choosing: the torn-read tests need a `Varalias` cell
/// whose word looks like a heap value, which no real transition produces.
fn stage(sym: &mut LispSymbol, redirect: SymbolRedirect, word: Value) {
    sym.flags.set_redirect(redirect);
    let p = std::ptr::from_mut(&mut sym.val.0).cast::<std::sync::atomic::AtomicUsize>();
    // SAFETY: the word is a `usize`; the test thread owns the writer side.
    unsafe { (*p).store(word.bits(), std::sync::atomic::Ordering::Release) };
}

/// Drive the shared writer loop: flip the symbol between
///   State P: redirect=Plainval, val word = HEAP_A
///   State V: redirect=Varalias, val word = HEAP_B (deliberately staged as a
///            heap-looking word so a TORN (Plainval, HEAP_B) read is detectable;
///            a real SymId alias word would be non-heap and silently invisible)
/// EXACTLY mirroring `CellWrite`: open the window (`seqlock_enter`: odd count,
/// Release fence), do the two writes (redirect first, then the val word — so a
/// non-retrying reader that samples redirect=Plainval then the
/// still-stale/just-updated word can tear), close it (`seqlock_exit`).
fn run_seqlock_writer(shared: Shared, done: &AtomicBool) {
    use std::sync::atomic::Ordering;
    let sym: &mut LispSymbol = unsafe { &mut *shared.0 };
    let seq: &AtomicU32 = unsafe { &*shared.1 };
    let a = heap_a();
    let b = heap_b();
    for _ in 0..SEQLOCK_WRITER_ITERS {
        // --- State V: Varalias arm, word staged as HEAP_B ---
        seqlock_enter(seq); // -> odd: arm change in flight
        stage(sym, SymbolRedirect::Varalias, b);
        seqlock_exit(seq); // -> even

        // --- State P: Plainval arm, word = HEAP_A ---
        seqlock_enter(seq); // -> odd
        stage(sym, SymbolRedirect::Plainval, a);
        seqlock_exit(seq); // -> even
    }
    done.store(true, Ordering::Release);
}

fn run_paused_seqlock_writer(
    shared: Shared,
    arm_published: &std::sync::Barrier,
    reader_sampled: &std::sync::Barrier,
) {
    let sym: &mut LispSymbol = unsafe { &mut *shared.0 };
    let seq: &AtomicU32 = unsafe { &*shared.1 };

    seqlock_enter(seq); // odd: arm change in flight
    sym.flags.set_redirect(SymbolRedirect::Plainval);
    arm_published.wait();
    reader_sampled.wait();
    stage(sym, SymbolRedirect::Plainval, heap_a());
    seqlock_exit(seq); // even: stable State P
}

#[test]
fn seqlock_symbol_read_never_tears_arm() {
    crate::test_utils::init_test_tracing();
    use std::sync::atomic::Ordering;

    // Start in State P so the very first reads (before the writer runs) are
    // already a consistent Plainval/HEAP_A pair.
    let mut sym = LispSymbol::new(intern("vm-seqlock-test-sym"));
    stage(&mut sym, SymbolRedirect::Plainval, heap_a());
    // Only the val arm may produce a child: function/plist are NIL (non-heap).
    sym.function = Value::NIL;
    sym.plist = Value::NIL;

    let seq = AtomicU32::new(0); // even = stable
    let done = AtomicBool::new(false);

    let a = heap_a();
    let b = heap_b();

    std::thread::scope(|scope| {
        let shared = Shared(&mut sym as *mut LispSymbol, &seq as *const _);
        let writer = scope.spawn(|| run_seqlock_writer(shared, &done));

        // Reader (this thread): hammer the real protocol until the writer is done.
        // Every pushed child MUST be HEAP_A — the only value legally reachable
        // through the Plainval arm. If HEAP_B (the Varalias-arm word) is ever
        // pushed, the seqlock failed to prevent a torn-arm read.
        let mut reads: u64 = 0;
        while !done.load(Ordering::Acquire) {
            for _ in 0..1024 {
                read_symbol_children_consistent(&seq, &sym, |child| {
                    assert_eq!(
                        child.bits(),
                        a.bits(),
                        "TORN ARM READ: protocol pushed {:#x}, expected HEAP_A {:#x} \
                         (HEAP_B is {:#x} — pushing it means redirect=Plainval was \
                         paired with the Varalias-arm word)",
                        child.bits(),
                        a.bits(),
                        b.bits(),
                    );
                });
                reads += 1;
            }
        }
        writer.join().unwrap();
        // Sanity: the reader actually ran many times against the live race.
        assert!(reads > 1000, "reader barely ran ({reads} iterations)");
    });
}

#[test]
fn seqlock_negative_control_tears_without_protocol() {
    crate::test_utils::init_test_tracing();
    use std::sync::Barrier;

    // Begin in State V. The writer will publish the Plainval redirect, then
    // pause before replacing HEAP_B with HEAP_A so the reader deterministically
    // samples the exact torn window that the seqlock protects.
    let mut sym = LispSymbol::new(intern("vm-seqlock-test-sym"));
    stage(&mut sym, SymbolRedirect::Varalias, heap_b());
    sym.function = Value::NIL;
    sym.plist = Value::NIL;

    let seq = AtomicU32::new(0);
    let arm_published = Barrier::new(2);
    let reader_sampled = Barrier::new(2);
    let b = heap_b();

    let (redirect, value) = std::thread::scope(|scope| {
        let shared = Shared(&mut sym as *mut LispSymbol, &seq as *const _);
        let writer =
            scope.spawn(|| run_paused_seqlock_writer(shared, &arm_published, &reader_sampled));

        // BROKEN reader: read the redirect tag, then read the val word, with NO
        // seqlock retry (no odd-check, no re-read of seq). This is exactly the
        // bug the real protocol defends against. The barriers hold the writer
        // mid-flip V->P: redirect is Plainval while the word is still HEAP_B.
        arm_published.wait();
        let redirect = sym.flags.load_redirect();
        let value = Value::from_bits(sym.load_word_acquire());
        reader_sampled.wait();
        writer.join().unwrap();
        (redirect, value)
    });

    assert_eq!(redirect, SymbolRedirect::Plainval);
    assert!(value.is_heap_object());
    assert_eq!(value.bits(), b.bits(), "broken reader must accept HEAP_B");
}
