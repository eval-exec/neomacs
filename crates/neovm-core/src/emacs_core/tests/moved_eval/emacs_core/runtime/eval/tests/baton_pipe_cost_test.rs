//! The pipe-readiness shape of the Baton thread-cost rows
//! (`neovm-eval`'s `baton_cost_micro_test.rs`, `baton_blocking_io_peer_progress`).
//!
//! Main writes a line to a `cat` pipe process and waits in
//! `accept-process-output` for the echo while a peer thread keeps asking for
//! the Baton. Each round costs the send, the native wait (which releases the
//! Baton to the peer), the readiness wake and the Baton's return to main. The
//! eval crate's unit runtime has no process subrs, so the row lives here, on
//! core's bare evaluator, where `make-process` is the `process.c` subr.
//!
//! Knobs, as in the eval rows:
//! - `TBC_MICRO_N`: the iteration count (default 200).
//! - `TBC_ROUTE`: only `green` (the default) until the default switch; a Sim
//!   World has no peer to run during the wait.
//!
//! The row prints one `baton_row=` line with the timed section's elapsed
//! nanoseconds and the number of rounds in which the peer ran during the wait
//! (an echo that is already readable when main reaches the wait need not
//! release the Baton, so that count is reported, not asserted).

use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn iterations() -> usize {
    std::env::var("TBC_MICRO_N")
        .ok()
        .map(|n| n.parse().expect("TBC_MICRO_N must be a count"))
        .unwrap_or(200)
}

fn green_route() -> Context {
    match std::env::var("TBC_ROUTE").as_deref() {
        Ok("green") | Err(_) => {}
        Ok(other) => panic!("TBC_ROUTE must be green until the default switch, got {other:?}"),
    }
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    crate::test_utils::new_green_context()
}

/// Waits that may return nil (a second without output) before the row fails.
const NIL_RETURN_CAP: usize = 10;

/// `n` copies of BODY as one `while` loop over VAR (the bare evaluator has
/// no `dotimes`: `subr.el` is not loaded).
fn repeat(n: usize, var: &str, body: &str) -> String {
    format!("(let (({var} 0)) (while (< {var} {n}) {body} (setq {var} (1+ {var}))))")
}

/// Main makes N pipe round trips through `cat` while a peer runs.
#[test]
fn baton_pipe_readiness_peer_progress() {
    pipe_round_trips("baton_pipe_readiness_peer_progress", Peer::Yielding);
}

/// The same N round trips with no peer: subtract it for the Baton part.
#[test]
fn baton_pipe_readiness_control() {
    pipe_round_trips("baton_pipe_readiness_control", Peer::Absent);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Peer {
    Yielding,
    Absent,
}

fn pipe_round_trips(row: &str, peer: Peer) {
    let n = iterations();
    let mut ev = green_route();
    let peer_form = match peer {
        Peer::Yielding => {
            "(setq bpr-th (make-thread
                           (lambda ()
                             (while (null bpr-stop)
                               (setq bpr-turns (1+ bpr-turns))
                               (thread-yield)))))"
        }
        Peer::Absent => "(setq bpr-th nil)",
    };
    assert_eq!(
        eval(
            &mut ev,
            &format!(
                "(progn
                   (setq bpr-stop nil bpr-turns 0 bpr-got 0)
                   (setq bpr-p (make-process
                                :name \"bpr-cat\" :command '(\"cat\") :connection-type 'pipe
                                :noquery t
                                :filter (lambda (_proc string)
                                          (setq bpr-got (+ bpr-got (length string))))))
                   {peer_form}
                   (thread-yield)
                   (list (process-status bpr-p) (if bpr-th (thread-live-p bpr-th) t)))"
            )
        ),
        "(run t)",
        "the row needs a running cat and, with a peer, a live one"
    );
    // A wait that returns nil received no output for a whole second: under
    // a live cat that is a lost readiness wakeup, so the count is reported
    // and capped, and a dead cat ends the row instead of spinning.
    let measured = format!(
        "(let ((progressed 0) (want 0) (nils 0))
           {}
           (list bpr-got progressed nils))",
        repeat(
            n,
            "i",
            &format!(
                "(let ((before bpr-turns))
                   (setq want (+ want 2))
                   (process-send-string bpr-p \"x\\n\")
                   (while (< bpr-got want)
                     (if (accept-process-output bpr-p 1)
                         nil
                       (setq nils (1+ nils))
                       (if (or (> nils {NIL_RETURN_CAP}) (null (eq (process-status bpr-p) 'run)))
                           (signal 'error (list \"bpr: echo lost\" nils (process-status bpr-p))))))
                   (if (> bpr-turns before) (setq progressed (1+ progressed))))"
            )
        )
    );
    let start = std::time::Instant::now();
    let result = eval(&mut ev, &measured);
    let elapsed = start.elapsed().as_nanos();
    let fields: Vec<&str> = result
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .map(|list| list.split(' ').collect())
        .unwrap_or_default();
    let [got, progressed, nils] = fields[..] else {
        panic!("the timed section returned {result}");
    };
    println!(
        "baton_row={row} route=green n={n} elapsed_ns={elapsed} peer_progress_rounds={progressed} \
         nil_returns={nils}"
    );
    assert_eq!(got, (2 * n).to_string(), "every echo arrives");
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq bpr-stop t)
               (delete-process bpr-p)
               (list (and bpr-th (thread-join bpr-th)) (thread-last-error)))"
        ),
        "(nil nil)",
        "the peer, if any, ran to the end without an error"
    );
}
