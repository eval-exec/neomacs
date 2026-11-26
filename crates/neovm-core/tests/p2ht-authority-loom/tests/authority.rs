//! Authority-word cases: reader overlap, the writer drain, reader retry under
//! WRITER, keeper quiescence, and the torn-entry negative control for a
//! writer that skips the drain (`tmp/peer-p2ht/design.md` §2.1, §3.3).
#![forbid(unsafe_code)]

mod model;
mod support;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use loom::thread;

use model::{K1, K2, Table, V1, V2, WRITER, WriterLease, wait_for};

/// Two sealed readers overlap on one table and neither blocks the other:
/// reader B enters while reader A is inside, and observes count == 2.
/// A's exit is gated on B's entered flag, so B's observation happens with A
/// still in flight in every explored execution.
#[test]
fn readers_overlap_without_serialising() {
    support::check(3, || {
        let table = Arc::new(Table::new());
        let a_in = Arc::new(AtomicBool::new(false));
        let b_entered = Arc::new(AtomicBool::new(false));
        let overlap = Arc::new(AtomicBool::new(false));

        let (t, a, b) = (
            Arc::clone(&table),
            Arc::clone(&a_in),
            Arc::clone(&b_entered),
        );
        let reader_a = thread::spawn(move || {
            let lease = model::ReaderLease::enter(&t);
            a.store(true, Ordering::Release);
            wait_for(&b);
            drop(lease);
        });

        let (t, a, b, seen) = (
            Arc::clone(&table),
            Arc::clone(&a_in),
            Arc::clone(&b_entered),
            Arc::clone(&overlap),
        );
        let reader_b = thread::spawn(move || {
            wait_for(&a);
            // Enter must succeed under another reader: the count only refuses
            // WRITER. This call would loop forever if readers serialised.
            let lease = model::ReaderLease::enter(&t);
            let count = t.reader_count_now();
            if count >= 2 {
                seen.store(true, Ordering::Release);
            }
            b.store(true, Ordering::Release);
            drop(lease);
        });

        reader_a.join().unwrap();
        reader_b.join().unwrap();
        assert!(
            overlap.load(Ordering::Acquire),
            "no execution overlapped two readers"
        );
        assert_eq!(table.reader_count_now(), 0);
    });
}

/// The writer drain: a writer's CAS succeeds only from a fully quiescent
/// word, so by the time it mutates, the in-flight reader has copied the OLD
/// consistent entry and left. The reader never observes the mutation.
#[test]
fn writer_drains_readers_before_mutating() {
    support::check(3, || {
        let table = Arc::new(Table::new());
        let r_in = Arc::new(AtomicBool::new(false));
        let w_attempting = Arc::new(AtomicBool::new(false));
        let copied_key = Arc::new(AtomicUsize::new(K1));
        let copied_val = Arc::new(AtomicUsize::new(V1));

        let (t, r, w, ok, ov) = (
            Arc::clone(&table),
            Arc::clone(&r_in),
            Arc::clone(&w_attempting),
            Arc::clone(&copied_key),
            Arc::clone(&copied_val),
        );
        let reader = thread::spawn(move || {
            let lease = model::ReaderLease::enter(&t);
            r.store(true, Ordering::Release);
            wait_for(&w);
            let (key, value) = lease.copy_entry();
            ok.store(key, Ordering::Release);
            ov.store(value, Ordering::Release);
            drop(lease);
        });

        let (t, r, w) = (
            Arc::clone(&table),
            Arc::clone(&r_in),
            Arc::clone(&w_attempting),
        );
        let writer = thread::spawn(move || {
            wait_for(&r);
            w.store(true, Ordering::Release);
            let lease = WriterLease::enter(&t);
            lease.mutate();
            drop(lease);
        });

        reader.join().unwrap();
        writer.join().unwrap();
        assert_eq!(copied_key.load(Ordering::Acquire), K1);
        assert_eq!(copied_val.load(Ordering::Acquire), V1);
        assert_eq!(
            (
                table.key_word.load(Ordering::Acquire),
                table.val_word.load(Ordering::Acquire)
            ),
            (K2, V2)
        );
    });
}

/// Reader retry under WRITER: a reader that arrives while a writer holds the
/// word refuses, yields, and enters only after the writer releases — and
/// then necessarily observes the writer's published state, never a torn one.
#[test]
fn reader_retries_under_writer_then_sees_published_state() {
    support::check(3, || {
        let table = Arc::new(Table::new());
        let w_holding = Arc::new(AtomicBool::new(false));
        let copied_key = Arc::new(AtomicUsize::new(K1));
        let copied_val = Arc::new(AtomicUsize::new(V1));

        let (t, h) = (Arc::clone(&table), Arc::clone(&w_holding));
        let writer = thread::spawn(move || {
            let lease = WriterLease::enter(&t);
            h.store(true, Ordering::Release);
            lease.mutate();
            drop(lease);
        });

        let (t, h, ok, ov) = (
            Arc::clone(&table),
            Arc::clone(&w_holding),
            Arc::clone(&copied_key),
            Arc::clone(&copied_val),
        );
        let reader = thread::spawn(move || {
            wait_for(&h);
            // The enter below is the retry loop: while WRITER is set it
            // refuses; loom explores both the immediate and retried paths.
            let lease = model::ReaderLease::enter(&t);
            let (key, value) = lease.copy_entry();
            ok.store(key, Ordering::Release);
            ov.store(value, Ordering::Release);
            drop(lease);
        });

        writer.join().unwrap();
        reader.join().unwrap();
        assert_eq!(copied_key.load(Ordering::Acquire), K2);
        assert_eq!(copied_val.load(Ordering::Acquire), V2);
    });
}

/// Keeper quiescence: after every actor joins (the modeled AllStopped), the
/// authority word and the guard depth are zero, and the keeper may then
/// mutate without any authority — the KeeperTableMut license (§2.2). The
/// actors are sequenced (reader, writer, reader) because this case is about
/// the post-stop state, not the race; the racing coverage lives in the
/// drain, overlap and growth cases.
#[test]
fn keeper_quiescent_after_all_actors_stop() {
    support::check(4, || {
        let table = Arc::new(Table::new());
        let r1_done = Arc::new(AtomicBool::new(false));
        let w_done = Arc::new(AtomicBool::new(false));

        let (t, done) = (Arc::clone(&table), Arc::clone(&r1_done));
        let reader_one = thread::spawn(move || {
            let lease = model::ReaderLease::enter(&t);
            assert!(Table::entry_consistent(lease.copy_entry()));
            drop(lease);
            done.store(true, Ordering::Release);
        });

        let (t, r1, w) = (
            Arc::clone(&table),
            Arc::clone(&r1_done),
            Arc::clone(&w_done),
        );
        let writer = thread::spawn(move || {
            wait_for(&r1);
            let lease = WriterLease::enter(&t);
            lease.mutate();
            drop(lease);
            w.store(true, Ordering::Release);
        });

        let (t, w) = (Arc::clone(&table), Arc::clone(&w_done));
        let reader_two = thread::spawn(move || {
            wait_for(&w);
            let lease = model::ReaderLease::enter(&t);
            assert!(Table::entry_consistent(lease.copy_entry()));
            drop(lease);
        });

        reader_one.join().unwrap();
        writer.join().unwrap();
        reader_two.join().unwrap();

        // AllStopped: nothing else can run, so the word must be quiescent.
        assert_eq!(table.authority.load(Ordering::Acquire), 0);
        assert_eq!(table.guard_depth.load(Ordering::Acquire), 0);

        // Keeper license: mutate with no authority taken at all.
        table.writer_remove();
        assert_eq!(
            (
                table.key_word.load(Ordering::Acquire),
                table.val_word.load(Ordering::Acquire)
            ),
            (0, 0)
        );
    });
}

/// NEGATIVE control — a writer that force-sets WRITER without draining.
/// The in-flight reader's two word loads then straddle the writer's two
/// stores, and the copied pair is torn: `(K1, V2)` or `(K2, V1)`. The reader
/// here is hand-rolled on the raw word so the ONLY assertion that can fire
/// is the torn-entry check.
#[test]
fn writer_skipping_drain_observes_torn_entry() {
    const EXPECTED: &str = "torn entry observed";
    support::negative(3, EXPECTED, || {
        let table = Arc::new(Table::new());
        let r_in = Arc::new(AtomicBool::new(false));
        let r_done = Arc::new(AtomicBool::new(false));

        let (t, r, done) = (Arc::clone(&table), Arc::clone(&r_in), Arc::clone(&r_done));
        let reader = thread::spawn(move || {
            // Reader enter/exit without the model's invariants, so the torn
            // check is the sole assertion in this actor.
            let mut spins = 0;
            loop {
                let word = t.authority.load(Ordering::Acquire);
                let got = t.authority.compare_exchange_weak(
                    word,
                    word + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                if got.is_ok() {
                    break;
                }
                spins += 1;
                assert!(spins < model::SPIN_CAP, "reader spin cap exceeded");
                loom::thread::yield_now();
            }
            r.store(true, Ordering::Release);
            let (key, value) = t.copy_entry();
            if !Table::entry_consistent((key, value)) {
                panic!("{}", EXPECTED);
            }
            t.authority.fetch_sub(1, Ordering::Release);
            done.store(true, Ordering::Release);
        });

        let (t, r, done) = (Arc::clone(&table), Arc::clone(&r_in), Arc::clone(&r_done));
        let writer = thread::spawn(move || {
            wait_for(&r);
            // Buggy force-take: set WRITER alongside the live reader count,
            // never waiting for the drain.
            let word = t.authority.load(Ordering::Acquire);
            t.authority.store(word | WRITER, Ordering::Release);
            t.epoch.fetch_add(1, Ordering::AcqRel);
            t.key_word.store(K2, Ordering::Release);
            t.val_word.store(V2, Ordering::Release);
            t.epoch.fetch_add(1, Ordering::AcqRel);
            wait_for(&done);
            t.authority.store(0, Ordering::Release);
        });

        reader.join().unwrap();
        writer.join().unwrap();
    });
}

/// A reader entering an unhydrated table loads the payload generation only
/// after it holds the reader count or WRITER, while a writer replaces that
/// payload. Every observation is an even generation. The reader drops the
/// count before the test returns so the writer's drain can finish.
#[test]
fn reader_enter_observes_unhydrated_payload_only_under_a_word() {
    support::check(3, || {
        let table = Arc::new(Table::new());
        table.needs_hydration.store(true, Ordering::Release);

        let t = Arc::clone(&table);
        let reader = thread::spawn(move || {
            t.reader_enter_hydrating();
        });

        let t = Arc::clone(&table);
        let writer = thread::spawn(move || {
            let lease = WriterLease::enter(&t);
            t.replace_payload();
            drop(lease);
        });

        reader.join().unwrap();
        writer.join().unwrap();
        assert_eq!(table.authority.load(Ordering::Acquire), 0);
        assert_eq!(table.pending.load(Ordering::Acquire) % 2, 0);
        assert!(!table.needs_hydration.load(Ordering::Acquire));
    });
}

/// NEGATIVE control — the reader loads the payload generation before taking
/// a word. The writer's odd window is then visible. The reader takes no
/// count, so the writer's drain is not the assertion under test.
#[test]
fn reader_loading_payload_before_the_word_sees_a_mid_replace() {
    const EXPECTED: &str = "unsynchronized payload observed";
    support::negative(3, EXPECTED, || {
        let table = Arc::new(Table::new());
        table.needs_hydration.store(true, Ordering::Release);

        let odd = Arc::new(AtomicBool::new(false));
        let (t, flag) = (Arc::clone(&table), Arc::clone(&odd));
        let writer = thread::spawn(move || {
            let lease = WriterLease::enter(&t);
            let generation = t.pending.load(Ordering::Acquire);
            t.pending.store(generation + 1, Ordering::Release);
            flag.store(true, Ordering::Release);
            loom::thread::yield_now();
            t.pending.store(generation + 2, Ordering::Release);
            t.needs_hydration.store(false, Ordering::Release);
            drop(lease);
        });

        let (t, flag) = (Arc::clone(&table), Arc::clone(&odd));
        let reader = thread::spawn(move || {
            // No authority word: the load sits in the writer's odd window.
            wait_for(&flag);
            let generation = t.pending.load(Ordering::Acquire);
            if generation % 2 == 1 {
                panic!("{}", EXPECTED);
            }
        });

        reader.join().unwrap();
        writer.join().unwrap();
    });
}

/// Sanity for the word layout constants used above.
#[test]
fn word_layout_constants() {
    assert_eq!(WRITER & model::COUNT_MASK, 0);
    assert_eq!(WRITER | (model::COUNT_MASK & 3), WRITER | 3);
}

/// B4: three readers overlap on one table — the entry order is chained
/// (A, then B once A is in, then C once B is in) over blocking latches, the
/// same sequencing `keeper_quiescent_after_all_actors_stop` and the guard
/// cases use, because the racing concurrent-entry coverage already lives in
/// `readers_overlap_without_serialising`; this case is about the
/// three-reader count and the drain. A writer then publishes its attempt
/// and waits in the drain. C observes the writer's wait with a nonzero
/// reader count and no WRITER bit (the design §6 drain witness), the three
/// readers leave, and only then does the writer's edit run on the fully
/// drained word. Four actors plus the main thread are beyond exhaustive
/// reach (an earlier spin-wait draft ran >30 min unbounded), so this one
/// case runs preemption-bounded (support::check_bounded, bound 2).
#[test]
fn three_readers_overlap_then_writer_drains_all() {
    support::check_bounded(5, 2, || {
        let table = Arc::new(Table::new());
        let a_in = Arc::new(model::Latch::new());
        let b_in = Arc::new(model::Latch::new());
        let c_in = Arc::new(model::Latch::new());
        let w_attempting = Arc::new(model::Latch::new());
        let overlap = Arc::new(AtomicBool::new(false));
        let drain = Arc::new(AtomicBool::new(false));

        let (t, a, c) = (Arc::clone(&table), Arc::clone(&a_in), Arc::clone(&c_in));
        let reader_a = thread::spawn(move || {
            let lease = model::ReaderLease::enter(&t);
            a.set();
            c.wait();
            drop(lease);
        });

        let (t, a, b, c) = (
            Arc::clone(&table),
            Arc::clone(&a_in),
            Arc::clone(&b_in),
            Arc::clone(&c_in),
        );
        let reader_b = thread::spawn(move || {
            a.wait();
            // Enter must succeed under A's live reader count.
            let lease = model::ReaderLease::enter(&t);
            b.set();
            c.wait();
            drop(lease);
        });

        let (t, b, c, w, seen, drain_seen) = (
            Arc::clone(&table),
            Arc::clone(&b_in),
            Arc::clone(&c_in),
            Arc::clone(&w_attempting),
            Arc::clone(&overlap),
            Arc::clone(&drain),
        );
        let reader_c = thread::spawn(move || {
            b.wait();
            // Enter must succeed under two live readers.
            let lease = model::ReaderLease::enter(&t);
            if t.reader_count_now() >= 3 {
                seen.store(true, Ordering::Release);
            }
            c.set();
            w.wait();
            // The drain witness: the writer is waiting to acquire while
            // this reader still holds its count.
            let word = t.authority.load(Ordering::Acquire);
            drain_seen.store(
                word & model::COUNT_MASK >= 1 && word & WRITER == 0,
                Ordering::Release,
            );
            drop(lease);
        });

        let (t, c, w) = (
            Arc::clone(&table),
            Arc::clone(&c_in),
            Arc::clone(&w_attempting),
        );
        let writer = thread::spawn(move || {
            c.wait();
            w.set();
            let lease = WriterLease::enter(&t);
            lease.mutate();
            drop(lease);
        });

        reader_a.join().unwrap();
        reader_b.join().unwrap();
        reader_c.join().unwrap();
        writer.join().unwrap();
        assert!(
            overlap.load(Ordering::Acquire),
            "no execution overlapped three readers"
        );
        assert!(
            drain.load(Ordering::Acquire),
            "no execution saw the writer waiting on a nonzero reader count"
        );
        assert_eq!(table.reader_count_now(), 0);
        assert_eq!(
            (
                table.key_word.load(Ordering::Acquire),
                table.val_word.load(Ordering::Acquire)
            ),
            (K2, V2)
        );
    });
}
