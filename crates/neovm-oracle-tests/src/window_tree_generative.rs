//! Generative window-tree parity: random operation sequences replayed in GNU
//! Emacs and neomacs, comparing the resulting `(window-tree)` after **every**
//! step.
//!
//! # Why generative
//!
//! Hand-written window-tree cases only cover the bugs someone already thought
//! of. Two concrete misses from the session that produced this file:
//!
//! - A 29-scenario hand-written battery reached byte-identical parity with GNU
//!   while `recombine_windows` was still entirely absent from the delete path.
//! - The first attempt to hand-write a reproducer for that gap *appeared to
//!   pass*, because the nesting was built with `window-combination-limit t` —
//!   which seals the new parent, and GNU skips sealed nodes. Only an
//!   *orthogonal* split produces the unsealed combination that triggers the
//!   merge.
//!
//! Neither is something you write a targeted test for before you understand the
//! bug; both fall out of random op sequences immediately.
//!
//! # What is compared
//!
//! For each step: whether the operation succeeded or signalled, and the full
//! window tree (combination direction, buffer names, window edges). Error
//! *messages* are deliberately not compared — the fact of an error is a
//! behavioural contract, the wording is not (neomacs embeds frame pointers in
//! some window errors, which would be pure noise here).
//!
//! Comparing after every step, rather than only at the end, means a failure
//! reports the first diverging operation instead of a final tree that has to be
//! reverse-engineered.

use proptest::prelude::*;

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{ORACLE_PROP_CASES, eval_oracle_and_neovm};

/// How many random sequences to run.
///
/// Defaults to the suite-wide [`ORACLE_PROP_CASES`], which is deliberately low
/// so the whole oracle corpus stays fast. Raise it with
/// `NEOVM_WINDOW_PROP_CASES` to hunt: this generator explores a far larger
/// space than a single scalar form does, so it earns a deeper run than a
/// typical parity property.
fn window_prop_cases() -> u32 {
    std::env::var("NEOVM_WINDOW_PROP_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(ORACLE_PROP_CASES)
}

/// The side argument of a split, and the edge of a side window.
#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
    Above,
    Below,
}

impl Side {
    fn split_arg(self) -> &'static str {
        match self {
            Side::Left => "'left",
            Side::Right => "'right",
            Side::Above => "'above",
            Side::Below => "'below",
        }
    }

    /// `window-side` parameter value; `above`/`below` are spelled
    /// `top`/`bottom` for side windows.
    fn side_window_arg(self) -> &'static str {
        match self {
            Side::Left => "'left",
            Side::Right => "'right",
            Side::Above => "'top",
            Side::Below => "'bottom",
        }
    }
}

fn side_strategy() -> impl Strategy<Value = Side> {
    prop_oneof![
        Just(Side::Left),
        Just(Side::Right),
        Just(Side::Above),
        Just(Side::Below),
    ]
}

/// The `window-combination-resize` binding in force for one operation.
///
/// This is a *policy* variable read by `window.el`, which stages the resulting
/// sizes for the primitive to commit. `t` makes a split take its space from
/// every sibling proportionally rather than only from the split target, and
/// makes a delete give the freed space back the same way.
#[derive(Debug, Clone, Copy)]
enum Resize {
    Nil,
    T,
    Side,
}

impl Resize {
    fn elisp(self) -> &'static str {
        match self {
            Resize::Nil => "nil",
            Resize::T => "t",
            Resize::Side => "'side",
        }
    }
}

fn resize_strategy() -> impl Strategy<Value = Resize> {
    // `nil` is the default and must stay the common case; `side` is what
    // `window--make-major-side-window` binds.
    prop_oneof![4 => Just(Resize::Nil), 2 => Just(Resize::T), 1 => Just(Resize::Side)]
}

/// One window-tree mutation. `window` indexes into
/// `(window-list nil 'no-minibuf nil)` modulo its length, so every op applies
/// to *some* live window regardless of how the tree has evolved.
#[derive(Debug, Clone, Copy)]
enum WindowOp {
    /// `split-window`, with `window-combination-limit` and
    /// `window-combination-resize` bound around it.
    Split {
        window: usize,
        side: Side,
        limit: bool,
        resize: Resize,
    },
    /// `delete-window`, with `window-combination-resize` bound around it.
    Delete { window: usize, resize: Resize },
    /// `display-buffer-in-side-window` on the given edge.
    SideWindow { side: Side, slot: i8 },
    /// `set-window-combination-limit` on a window's parent — the slot that
    /// decides whether a delete may recombine it.
    SealParent { window: usize, value: bool },
    /// `delete-other-windows`.
    DeleteOtherWindows { window: usize },
    /// `balance-windows`.
    Balance,
    /// `window-make-atom` on a window's parent: the subtree then resizes and
    /// deletes as a unit, and `split-window` redirects to the atom's root.
    MakeAtom { window: usize },
    /// `window-resize` by an explicit delta — the path that stages sizes
    /// through `window--resize-child-windows` before committing them.
    Resize {
        window: usize,
        delta: i8,
        horizontal: bool,
    },
    /// Save the current configuration for a later `Restore`.
    SaveConfiguration,
    /// `set-window-configuration` of the most recent `SaveConfiguration`
    /// (no-op if none was taken). Rebuilds the whole tree from a saved record,
    /// which exercises a completely different construction path from splitting.
    RestoreConfiguration,
    /// `fit-window-to-buffer`.
    FitToBuffer { window: usize },
    /// `shrink-window-if-larger-than-buffer`.
    ShrinkIfLarger { window: usize },
    /// Bind `window-sides-slots` so side-window creation hits the slot cap.
    SetSidesSlots { slots: i8 },
    /// `enlarge-window` / `shrink-window` on the selected window — the
    /// interactive resize entry points, which stage through
    /// `window--resize-this-window` like everything else.
    EnlargeOrShrink {
        window: usize,
        delta: i8,
        horizontal: bool,
        shrink: bool,
    },
    /// `maximize-window` / `minimize-window`: resize to the extreme the tree
    /// allows, which stresses the min/max clamping in the staging pass.
    MaximizeOrMinimize { window: usize, minimize: bool },
}

fn op_strategy() -> impl Strategy<Value = WindowOp> {
    prop_oneof![
        // Splits dominate: they are what builds interesting trees.
        4 => (0usize..8, side_strategy(), any::<bool>(), resize_strategy())
            .prop_map(|(window, side, limit, resize)| WindowOp::Split { window, side, limit, resize }),
        3 => (0usize..8, resize_strategy())
            .prop_map(|(window, resize)| WindowOp::Delete { window, resize }),
        3 => (side_strategy(), -1i8..2)
            .prop_map(|(side, slot)| WindowOp::SideWindow { side, slot }),
        1 => (0usize..8, any::<bool>())
            .prop_map(|(window, value)| WindowOp::SealParent { window, value }),
        1 => (0usize..8).prop_map(|window| WindowOp::DeleteOtherWindows { window }),
        1 => Just(WindowOp::Balance),
        1 => (0usize..8).prop_map(|window| WindowOp::MakeAtom { window }),
        2 => (0usize..8, -8i8..9, any::<bool>())
            .prop_map(|(window, delta, horizontal)| WindowOp::Resize { window, delta, horizontal }),
        1 => Just(WindowOp::SaveConfiguration),
        1 => Just(WindowOp::RestoreConfiguration),
        1 => (0usize..8).prop_map(|window| WindowOp::FitToBuffer { window }),
        1 => (0usize..8).prop_map(|window| WindowOp::ShrinkIfLarger { window }),
        1 => (-1i8..4).prop_map(|slots| WindowOp::SetSidesSlots { slots }),
        2 => (0usize..8, -6i8..7, any::<bool>(), any::<bool>())
            .prop_map(|(window, delta, horizontal, shrink)| WindowOp::EnlargeOrShrink {
                window, delta, horizontal, shrink }),
        1 => (0usize..8, any::<bool>())
            .prop_map(|(window, minimize)| WindowOp::MaximizeOrMinimize { window, minimize }),
    ]
}

/// The elisp body for one op. `step` disambiguates side-window buffer names so
/// a later side window does not silently reuse an earlier one's slot.
fn op_elisp(op: WindowOp, step: usize) -> String {
    match op {
        WindowOp::Split {
            window,
            side,
            limit,
            resize,
        } => format!(
            "(let ((window-combination-limit {}) (window-combination-resize {})) \
               (split-window (oracle--nth-window {window}) nil {}))",
            if limit { "t" } else { "nil" },
            resize.elisp(),
            side.split_arg(),
        ),
        WindowOp::Delete { window, resize } => format!(
            "(let ((window-combination-resize {})) \
               (delete-window (oracle--nth-window {window})))",
            resize.elisp(),
        ),
        WindowOp::SideWindow { side, slot } => format!(
            "(display-buffer (get-buffer-create \"*side-{step}*\") \
               (list 'display-buffer-in-side-window \
                     (cons 'side {}) (cons 'slot {slot})))",
            side.side_window_arg(),
        ),
        WindowOp::SealParent { window, value } => format!(
            "(set-window-combination-limit \
               (window-parent (oracle--nth-window {window})) {})",
            if value { "t" } else { "nil" },
        ),
        WindowOp::DeleteOtherWindows { window } => {
            format!("(delete-other-windows (oracle--nth-window {window}))")
        }
        WindowOp::Balance => "(balance-windows)".to_string(),
        WindowOp::MakeAtom { window } => {
            format!("(window-make-atom (window-parent (oracle--nth-window {window})))")
        }
        WindowOp::Resize {
            window,
            delta,
            horizontal,
        } => format!(
            "(window-resize (oracle--nth-window {window}) {delta} {})",
            if horizontal { "t" } else { "nil" },
        ),
        WindowOp::SaveConfiguration => {
            "(setq oracle--config (current-window-configuration))".to_string()
        }
        WindowOp::RestoreConfiguration => {
            "(when oracle--config (set-window-configuration oracle--config))".to_string()
        }
        WindowOp::FitToBuffer { window } => {
            format!("(fit-window-to-buffer (oracle--nth-window {window}))")
        }
        WindowOp::ShrinkIfLarger { window } => format!(
            "(with-selected-window (oracle--nth-window {window}) \
               (shrink-window-if-larger-than-buffer))"
        ),
        // `window-sides-slots' is a 4-element list of nil (unlimited) or a
        // non-negative slot count per side; -1 stands for nil here.
        WindowOp::SetSidesSlots { slots } => {
            let value = if slots < 0 {
                "nil".to_string()
            } else {
                slots.to_string()
            };
            format!("(setq window-sides-slots (list {value} {value} {value} {value}))")
        }
        WindowOp::EnlargeOrShrink {
            window,
            delta,
            horizontal,
            shrink,
        } => format!(
            "(with-selected-window (oracle--nth-window {window}) ({} {delta} {}))",
            if shrink {
                "shrink-window"
            } else {
                "enlarge-window"
            },
            if horizontal { "t" } else { "nil" },
        ),
        WindowOp::MaximizeOrMinimize { window, minimize } => format!(
            "({} (oracle--nth-window {window}))",
            if minimize {
                "minimize-window"
            } else {
                "maximize-window"
            },
        ),
    }
}

/// Build the full elisp program: run each op inside `condition-case`, and after
/// each one record `ok`/`err` plus the whole tree.
fn program(ops: &[WindowOp]) -> String {
    let mut src = String::from(
        r#"
(defun oracle--wt (node)
  (cond
   ((windowp node)
    (cons (buffer-name (window-buffer node)) (window-edges node)))
   ((consp node)
    (cons (if (car node) 'v 'h) (mapcar #'oracle--wt (cddr node))))))
(defun oracle--nth-window (n)
  (let ((ws (window-list nil 'no-minibuf nil)))
    (nth (mod n (length ws)) ws)))
(defvar oracle--log nil)
(defvar oracle--config nil)
(defmacro oracle--step (&rest body)
  `(setq oracle--log
         (cons (list (condition-case nil (progn ,@body 'ok) (error 'err))
                     (oracle--wt (car (window-tree))))
               oracle--log)))
"#,
    );
    for (step, op) in ops.iter().enumerate() {
        src.push_str(&format!("(oracle--step {})\n", op_elisp(*op, step)));
    }
    src.push_str("(nreverse oracle--log)\n");
    src
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(window_prop_cases()))]

    /// Random split/delete/side-window/seal/balance sequences must leave GNU and
    /// neomacs with identical window trees at every step.
    ///
    /// # Depth
    ///
    /// Runs at the suite-wide [`ORACLE_PROP_CASES`] by default so it stays
    /// cheap enough to gate every commit. That shallow run only catches gross
    /// regressions — to actually *hunt*, raise the depth:
    ///
    /// ```text
    /// NEOVM_FORCE_ORACLE_PATH=/path/to/emacs NEOVM_ORACLE_MODE=live \
    /// NEOVM_WINDOW_PROP_CASES=800 \
    /// cargo nextest run --release -p neovm-oracle-tests \
    ///   -E 'test(oracle_prop_window_tree_survives_random_operation_sequences)' --no-capture
    /// ```
    ///
    /// # A note on flakiness
    ///
    /// This test once gave FAIL / PASS / FAIL on identical runs, which looked
    /// like harness flakiness. It was not: neomacs itself produced two
    /// different window trees for the same input about 10% of the time,
    /// because `Window`'s GC trace omitted the heap floats holding
    /// `normal_cols`/`normal_lines`, so a collection could free a live
    /// window's proportional sizes. Fixed; the seed that exposed it is now
    /// 40/40 stable and matches GNU.
    ///
    /// The lesson for the next apparent flake: test each engine against
    /// ITSELF before blaming the harness. Running one generated program N
    /// times per engine and comparing each engine's outputs to its own
    /// separates "nondeterministic engine" from "bad comparison" in a single
    /// step.
    ///
    /// Note `proptest-regressions/` is GITIGNORED here (`.gitignore:438`), so
    /// seeds are local-only and a deep run must rediscover cases each time.
    #[test]
    fn oracle_prop_window_tree_survives_random_operation_sequences(
        ops in prop::collection::vec(op_strategy(), 2..9),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = program(&ops);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(
            &oracle,
            &neovm,
            "window tree diverged from GNU for op sequence {:?}\nprogram:\n{}",
            ops,
            form
        );
    }
}
