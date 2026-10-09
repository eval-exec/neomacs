//! A soak on GNU's own Lisp loaded from source: every function of pcase,
//! cl-seq, cl-extra, seq, subr-x and ring is interpreted, so the workload
//! runs thousands of compiled bodies (at threshold 1) in `verify` mode, and
//! must print what the tree walker prints.

use crate::emacs_core::eval::{Context, TierIEvent, TierIMode};
use crate::emacs_core::print::print_value;

const LOAD: &str = "(let ((load-suffixes '(\".el\")))
  (dolist (lib '(\"emacs-lisp/pcase\" \"emacs-lisp/cl-seq\" \"emacs-lisp/cl-extra\"
                 \"emacs-lisp/seq\" \"emacs-lisp/subr-x\" \"emacs-lisp/ring\"))
    (load lib nil t)))";

const WORK: &str = r#"(let ((acc nil))
  (dotimes (i 12)
    (push (list (seq-filter (lambda (x) (> x i)) '(1 5 10 20 40))
                (cl-remove-if (lambda (x) (= x i)) (number-sequence 0 5))
                (seq-reduce (lambda (a b) (+ a b i)) '(1 2 3) 0)
                (cl-some (lambda (x) (and (> x i) x)) '(3 7 11))
                (seq-map-indexed (lambda (e n) (cons e (+ n i))) '(a b))
                (let ((r (make-ring 3))) (ring-insert r i) (ring-insert r (* i i)) (ring-elements r))
                (string-join (mapcar (lambda (s) (format "%s%d" s i)) '("a" "b")) ",")
                (cl-sort (list 3 i 1 (- i)) #'<)
                (seq-uniq (list i 1 i 2))
                (string-trim (format "  %d  " i))
                (macroexpand-all
                 `(pcase x
                    (`(,a . ,b) (list a b ,i))
                    ((pred stringp) 'str)
                    ((and (pred integerp) n (guard (> n ,i))) n)
                    (_ nil)))
                (pcase (list i 'x)
                  (`(,(and n (guard (cl-evenp n))) ,s) (list 'even n s))
                  (`(,n . ,_) (list 'odd n))))
          acc))
  acc)"#;

fn context(mode: TierIMode) -> Context {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.tier_i.set_mode(mode);
    eval.tier_i.set_threshold(1);
    eval.tier_i.clear_for_test();
    eval
}

fn printed(eval: &mut Context, src: &str) -> String {
    let value = eval
        .eval_str(src)
        .unwrap_or_else(|err| panic!("{src}: {err:?}"));
    print_value(&value)
}

#[test]
fn soak_on_gnu_sources_prints_what_the_tree_walker_prints() {
    crate::test_utils::init_test_tracing();
    let mut off = context(TierIMode::Off);
    printed(&mut off, LOAD);
    let expected = printed(&mut off, WORK);

    let mut verify = context(TierIMode::Verify);
    printed(&mut verify, LOAD);
    assert_eq!(printed(&mut verify, WORK), expected);
    let stats = verify.tier_i.stats();
    tracing::info!("tier-i soak: {}", stats.report());
    assert!(stats.count(TierIEvent::Run) > 1000, "{}", stats.report());
    assert!(
        stats.count(TierIEvent::Compiled) > 100,
        "{}",
        stats.report()
    );

    let mut on = context(TierIMode::On);
    printed(&mut on, LOAD);
    assert_eq!(printed(&mut on, WORK), expected);
}

/// Not a check: the soak's workload `TIER_I_BENCH_ROUNDS` times in the mode
/// `TIER_I_BENCH_MODE` names (`NEOVM_TIER_I` syntax), for `perf stat` on the
/// test binary.  `cargo nextest run -p neovm-core --run-ignored only -E
/// 'test(/tier_i_bench_workload/)'` with the variables set.
#[test]
#[ignore = "a measurement harness, run by hand"]
fn tier_i_bench_workload() {
    crate::test_utils::init_test_tracing();
    let mode = crate::emacs_core::eval::parse_tier_i_knob(
        std::env::var("TIER_I_BENCH_MODE").ok().as_deref(),
    );
    let rounds: usize = std::env::var("TIER_I_BENCH_ROUNDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let mut eval = context(mode);
    eval.tier_i.set_threshold(2);
    printed(&mut eval, LOAD);
    for _ in 0..rounds {
        printed(&mut eval, WORK);
    }
    tracing::info!("tier-i bench: {:?} {}", mode, eval.tier_i.stats().report());
}
